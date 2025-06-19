//! 稀疏向量索引和BM25搜索引擎实现
//!
//! 本模块提供：
//! - 稀疏向量的倒排索引
//! - BM25算法实现
//! - 文本预处理和分词
//! - 高效的稀疏向量存储和查询

use crate::{
    errors::Result,
    types::{BM25Stats, DocumentSparseRepresentation, SparseVector},
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// 倒排索引中的条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvertedIndexEntry {
    /// 文档ID
    pub document_id: String,
    /// 词频 (Term Frequency)
    pub term_frequency: f32,
    /// 文档长度
    pub document_length: f32,
}

/// 稀疏向量索引
#[derive(Debug)]
pub struct SparseIndex {
    /// 倒排索引: term_id -> 包含该词的文档列表
    inverted_index: Arc<RwLock<HashMap<u32, Vec<InvertedIndexEntry>>>>,
    /// BM25 统计信息
    bm25_stats: Arc<RwLock<BM25Stats>>,
    /// BM25 参数
    bm25_params: BM25Parameters,
}

/// BM25 算法参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Parameters {
    /// k1 参数，控制词频饱和度，通常取值 1.2-2.0
    pub k1: f32,
    /// b 参数，控制文档长度归一化，通常取值 0.75
    pub b: f32,
}

impl Default for BM25Parameters {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

impl SparseIndex {
    /// 创建新的稀疏向量索引
    pub fn new(bm25_params: BM25Parameters) -> Self {
        Self {
            inverted_index: Arc::new(RwLock::new(HashMap::new())),
            bm25_stats: Arc::new(RwLock::new(BM25Stats {
                total_documents: 0,
                average_document_length: 0.0,
                vocabulary_size: 0,
                document_frequencies: HashMap::new(),
            })),
            bm25_params,
        }
    }

    /// 添加文档到稀疏索引
    pub fn add_document(&self, sparse_doc: &DocumentSparseRepresentation) -> Result<()> {
        let mut inverted_index = self.inverted_index.write();
        let mut stats = self.bm25_stats.write();

        // 更新倒排索引
        for (&term_id, &tf) in &sparse_doc.term_frequencies {
            let entry = InvertedIndexEntry {
                document_id: sparse_doc.document_id.clone(),
                term_frequency: tf,
                document_length: sparse_doc.document_length,
            };

            inverted_index.entry(term_id).or_default().push(entry);
        }

        // 更新文档频率统计
        for &term_id in sparse_doc.term_frequencies.keys() {
            *stats.document_frequencies.entry(term_id).or_insert(0) += 1;
        }

        // 更新全局统计信息
        stats.total_documents += 1;
        stats.vocabulary_size = stats.document_frequencies.len();

        // 重新计算平均文档长度
        let total_length: f32 = inverted_index
            .values()
            .flat_map(|entries| entries.iter())
            .map(|entry| entry.document_length)
            .sum();

        if stats.total_documents > 0 {
            stats.average_document_length = total_length / stats.total_documents as f32;
        }

        Ok(())
    }

    /// 删除文档
    pub fn remove_document(&self, document_id: &str) -> Result<bool> {
        let mut inverted_index = self.inverted_index.write();
        let mut stats = self.bm25_stats.write();
        let mut removed = false;

        // 从倒排索引中删除文档
        for (term_id, entries) in inverted_index.iter_mut() {
            if let Some(pos) = entries
                .iter()
                .position(|entry| entry.document_id == document_id)
            {
                entries.remove(pos);
                removed = true;

                // 如果这是最后一个包含该词的文档，更新文档频率
                if entries.is_empty() {
                    stats.document_frequencies.remove(term_id);
                }
            }
        }

        if removed {
            stats.total_documents = stats.total_documents.saturating_sub(1);
            stats.vocabulary_size = stats.document_frequencies.len();

            // 重新计算平均文档长度
            let total_length: f32 = inverted_index
                .values()
                .flat_map(|entries| entries.iter())
                .map(|entry| entry.document_length)
                .sum();

            if stats.total_documents > 0 {
                stats.average_document_length = total_length / stats.total_documents as f32;
            } else {
                stats.average_document_length = 0.0;
            }
        }

        Ok(removed)
    }

    /// 使用BM25算法搜索相关文档
    pub fn search_bm25(
        &self,
        query_vector: &SparseVector,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let inverted_index = self.inverted_index.read();
        let stats = self.bm25_stats.read();

        if stats.total_documents == 0 {
            return Ok(Vec::new());
        }

        let mut document_scores: HashMap<String, f32> = HashMap::new();

        // 对查询向量中的每个词计算BM25分数
        for (&term_id, &query_tf) in query_vector.indices.iter().zip(query_vector.values.iter()) {
            if let Some(doc_list) = inverted_index.get(&term_id) {
                let df = stats
                    .document_frequencies
                    .get(&term_id)
                    .copied()
                    .unwrap_or(1);
                let idf = self.calculate_idf(stats.total_documents, df);

                for entry in doc_list {
                    let bm25_score = self.calculate_bm25_score(
                        query_tf,
                        entry.term_frequency,
                        entry.document_length,
                        stats.average_document_length,
                        idf,
                    );

                    *document_scores
                        .entry(entry.document_id.clone())
                        .or_insert(0.0) += bm25_score;
                }
            }
        }

        // 按分数排序并返回前limit个结果
        let mut sorted_results: Vec<_> = document_scores.into_iter().collect();
        sorted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted_results.truncate(limit);

        Ok(sorted_results)
    }

    /// 计算IDF (Inverse Document Frequency)
    fn calculate_idf(&self, total_docs: usize, doc_freq: usize) -> f32 {
        ((total_docs as f32 - doc_freq as f32 + 0.5) / (doc_freq as f32 + 0.5)).ln()
    }

    /// 计算BM25分数
    fn calculate_bm25_score(
        &self,
        query_tf: f32,
        doc_tf: f32,
        doc_length: f32,
        avg_doc_length: f32,
        idf: f32,
    ) -> f32 {
        let k1 = self.bm25_params.k1;
        let b = self.bm25_params.b;

        let tf_component =
            (doc_tf * (k1 + 1.0)) / (doc_tf + k1 * (1.0 - b + b * (doc_length / avg_doc_length)));

        query_tf * tf_component * idf
    }

    /// 获取索引统计信息
    pub fn get_stats(&self) -> BM25Stats {
        self.bm25_stats.read().clone()
    }

    /// 清空索引
    pub fn clear(&self) {
        let mut inverted_index = self.inverted_index.write();
        let mut stats = self.bm25_stats.write();

        inverted_index.clear();
        *stats = BM25Stats {
            total_documents: 0,
            average_document_length: 0.0,
            vocabulary_size: 0,
            document_frequencies: HashMap::new(),
        };
    }

    /// 获取索引大小（估算内存使用）
    pub fn get_memory_usage_mb(&self) -> f64 {
        let inverted_index = self.inverted_index.read();
        let stats = self.bm25_stats.read();

        // 估算倒排索引的内存使用
        let index_size = inverted_index
            .iter()
            .map(|(_, entries)| {
                std::mem::size_of::<u32>() + // term_id
                entries.len() * std::mem::size_of::<InvertedIndexEntry>()
            })
            .sum::<usize>();

        // 估算统计信息的内存使用
        let stats_size = std::mem::size_of::<BM25Stats>()
            + stats.document_frequencies.len()
                * (std::mem::size_of::<u32>() + std::mem::size_of::<usize>());

        (index_size + stats_size) as f64 / (1024.0 * 1024.0)
    }
}

/// 简单的文本分词器
pub struct SimpleTokenizer {
    /// 停用词列表
    stop_words: std::collections::HashSet<String>,
}

impl SimpleTokenizer {
    /// 创建新的分词器
    pub fn new() -> Self {
        let stop_words = [
            "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "he", "in",
            "is", "it", "its", "of", "on", "that", "the", "to", "was", "will", "with", "的", "了",
            "在", "是", "有", "和", "与", "或", "但", "而", "这", "那", "一", "不", "也", "就",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self { stop_words }
    }

    /// 文本分词并返回词频统计
    pub fn tokenize(&self, text: &str) -> HashMap<String, f32> {
        let mut term_frequencies = HashMap::new();

        // 简单的分词：按空格和标点符号分割
        let tokens: Vec<String> = text
            .to_lowercase()
            .split_whitespace()
            .map(|word| {
                word.chars()
                    .filter(|c| c.is_alphanumeric() || c.is_ascii_alphabetic())
                    .collect::<String>()
            })
            .filter(|word| !word.is_empty() && word.len() > 1 && !self.stop_words.contains(word))
            .collect();

        let total_tokens = tokens.len() as f32;

        for token in tokens {
            *term_frequencies.entry(token).or_insert(0.0) += 1.0;
        }

        // 计算相对频率
        for freq in term_frequencies.values_mut() {
            *freq /= total_tokens;
        }

        term_frequencies
    }

    /// 构建词汇表并将词映射到ID
    pub fn build_vocabulary(&self, documents: &[&str]) -> HashMap<String, u32> {
        let mut vocabulary = std::collections::HashSet::new();

        for doc in documents {
            let tokens = self.tokenize(doc);
            vocabulary.extend(tokens.keys().cloned());
        }

        vocabulary
            .into_iter()
            .enumerate()
            .map(|(i, term)| (term, i as u32))
            .collect()
    }

    /// 将文档转换为稀疏向量
    pub fn document_to_sparse_vector(
        &self,
        document_id: &str,
        text: &str,
        vocabulary: &HashMap<String, u32>,
    ) -> Result<DocumentSparseRepresentation> {
        let term_frequencies = self.tokenize(text);
        let document_length = term_frequencies.values().sum::<f32>();

        let mut indices = Vec::new();
        let mut values = Vec::new();
        let mut tf_map = HashMap::new();

        for (term, freq) in term_frequencies {
            if let Some(&term_id) = vocabulary.get(&term) {
                indices.push(term_id);
                values.push(freq);
                tf_map.insert(term_id, freq);
            }
        }

        // 按索引排序
        let mut pairs: Vec<_> = indices.into_iter().zip(values).collect();
        pairs.sort_by_key(|&(idx, _)| idx);

        let (sorted_indices, sorted_values): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();

        let sparse_vector = SparseVector::new(sorted_indices, sorted_values, vocabulary.len())?;

        Ok(DocumentSparseRepresentation {
            document_id: document_id.to_string(),
            sparse_vector,
            document_length,
            term_frequencies: tf_map,
        })
    }
}

impl Default for SimpleTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_vector_operations() {
        let vec1 = SparseVector::new(vec![0, 2, 4], vec![1.0, 2.0, 3.0], 10).unwrap();

        let vec2 = SparseVector::new(vec![1, 2, 3], vec![1.0, 2.0, 1.0], 10).unwrap();

        assert_eq!(vec1.dot_product(&vec2), 4.0); // 只有索引2重叠: 2.0 * 2.0 = 4.0
        assert!(vec1.cosine_similarity(&vec2) > 0.0);
    }

    #[test]
    fn test_simple_tokenizer() {
        let tokenizer = SimpleTokenizer::new();
        let tokens = tokenizer.tokenize("This is a test document with some words.");

        assert!(tokens.contains_key("test"));
        assert!(tokens.contains_key("document"));
        assert!(!tokens.contains_key("is")); // 停用词应该被过滤
        assert!(!tokens.contains_key("a")); // 停用词应该被过滤
    }

    #[test]
    fn test_sparse_index() {
        let index = SparseIndex::new(BM25Parameters::default());
        let tokenizer = SimpleTokenizer::new();

        let docs = vec!["test document", "another test"];
        let vocabulary = tokenizer.build_vocabulary(&docs);

        let sparse_doc = tokenizer
            .document_to_sparse_vector("doc1", "test document", &vocabulary)
            .unwrap();

        index.add_document(&sparse_doc).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_documents, 1);
        assert!(stats.vocabulary_size > 0);
    }
}
