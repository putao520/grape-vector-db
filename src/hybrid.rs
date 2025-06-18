//! 混合搜索引擎实现
//! 
//! 本模块提供：
//! - 混合搜索引擎（密集向量 + 稀疏向量 + 文本搜索）
//! - RRF (Reciprocal Rank Fusion) 算法
//! - 多种分数融合策略
//! - 高级混合搜索功能

use crate::{
    types::{
        SparseVector, HybridSearchRequest, FusionStrategy, SearchResult, 
        ScoreBreakdown, DocumentRecord, FusionWeights, FusionPerformanceStats,
        QueryMetrics
    },
    sparse::{SparseIndex, SimpleTokenizer},
    index::HnswVectorIndex,
    storage::VectorStore,
    errors::{Result, VectorDbError},
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use serde::{Serialize, Deserialize};

/// 学习式融合模型
pub trait FusionModel: Send + Sync {
    /// 基于历史数据预测最佳权重
    fn predict_weights(&self, query: &str, context: &FusionContext) -> FusionWeights;
    /// 基于用户反馈更新模型
    fn update_model(&mut self, feedback: &QueryMetrics) -> Result<()>;
}

/// 融合上下文信息
#[derive(Debug, Clone)]
pub struct FusionContext {
    /// 查询类型（语义/关键词/混合）
    pub query_type: QueryType,
    /// 查询长度
    pub query_length: usize,
    /// 历史点击率
    pub historical_ctr: f64,
    /// 时间段（工作/休息时间等）
    pub time_context: TimeContext,
}

#[derive(Debug, Clone)]
pub enum QueryType {
    Semantic,    // 语义查询（适合密集向量）
    Keyword,     // 关键词查询（适合稀疏向量）
    Mixed,       // 混合查询
}

#[derive(Debug, Clone)]
pub enum TimeContext {
    WorkingHours,
    RestingHours,
    Weekend,
}

/// 简单的统计学习模型
#[derive(Debug, Clone)]
pub struct StatisticalFusionModel {
    /// 每种查询类型的最佳权重统计
    query_type_weights: HashMap<String, FusionWeights>,
    /// 权重调整的历史记录
    weight_history: Vec<(FusionWeights, f64)>, // (权重, 用户满意度)
    /// 学习率
    learning_rate: f32,
}

impl StatisticalFusionModel {
    pub fn new(learning_rate: f32) -> Self {
        let mut query_type_weights = HashMap::new();
        
        // 为不同查询类型设置初始权重
        query_type_weights.insert("semantic".to_string(), FusionWeights {
            dense_weight: 0.8, sparse_weight: 0.15, text_weight: 0.05
        });
        query_type_weights.insert("keyword".to_string(), FusionWeights {
            dense_weight: 0.3, sparse_weight: 0.6, text_weight: 0.1
        });
        query_type_weights.insert("mixed".to_string(), FusionWeights {
            dense_weight: 0.5, sparse_weight: 0.4, text_weight: 0.1
        });
        
        Self {
            query_type_weights,
            weight_history: Vec::new(),
            learning_rate,
        }
    }
}

impl FusionModel for StatisticalFusionModel {
    fn predict_weights(&self, query: &str, context: &FusionContext) -> FusionWeights {
        let query_type_key = match context.query_type {
            QueryType::Semantic => "semantic",
            QueryType::Keyword => "keyword", 
            QueryType::Mixed => "mixed",
        };
        
        let base_weights = self.query_type_weights
            .get(query_type_key)
            .cloned()
            .unwrap_or_default();
        
        // 根据查询长度调整权重
        let length_factor = if context.query_length > 10 {
            1.2  // 长查询更适合语义搜索
        } else {
            0.8  // 短查询更适合关键词搜索
        };
        
        FusionWeights {
            dense_weight: (base_weights.dense_weight * length_factor).min(1.0),
            sparse_weight: base_weights.sparse_weight,
            text_weight: base_weights.text_weight,
        }
    }
    
    fn update_model(&mut self, feedback: &QueryMetrics) -> Result<()> {
        if let Some(satisfaction) = feedback.user_satisfaction {
            let satisfaction_score = satisfaction as f64 / 5.0; // 标准化到0-1
            
            // 简单的梯度下降更新
            if let Some(last_weights) = self.weight_history.last() {
                let weight_diff = satisfaction_score - last_weights.1;
                
                // 更新对应查询类型的权重
                for (_, weights) in self.query_type_weights.iter_mut() {
                    weights.dense_weight += self.learning_rate * weight_diff as f32;
                    weights.sparse_weight += self.learning_rate * weight_diff as f32 * 0.5;
                    weights.text_weight += self.learning_rate * weight_diff as f32 * 0.3;
                    
                    // 确保权重在合理范围内
                    weights.dense_weight = weights.dense_weight.clamp(0.1, 0.9);
                    weights.sparse_weight = weights.sparse_weight.clamp(0.1, 0.9);
                    weights.text_weight = weights.text_weight.clamp(0.05, 0.3);
                }
            }
            
            self.weight_history.push((FusionWeights::default(), satisfaction_score));
            
            // 限制历史记录大小
            if self.weight_history.len() > 1000 {
                self.weight_history.remove(0);
            }
        }
        
        Ok(())
    }
}

/// 混合搜索引擎
pub struct HybridSearchEngine {
    /// 密集向量搜索引擎
    dense_engine: Arc<HnswVectorIndex>,
    /// 稀疏向量搜索引擎
    sparse_engine: Arc<SparseIndex>,
    /// 文本分词器
    tokenizer: SimpleTokenizer,
    /// 词汇表
    vocabulary: Arc<parking_lot::RwLock<HashMap<String, u32>>>,
    /// 融合策略
    fusion_strategy: FusionStrategy,
    /// 学习式融合模型（可选）
    fusion_model: Option<Arc<Mutex<dyn FusionModel>>>,
    /// 性能统计
    performance_stats: Arc<Mutex<HashMap<String, FusionPerformanceStats>>>,
    /// 查询历史记录
    query_history: Arc<Mutex<Vec<QueryMetrics>>>,
}

impl HybridSearchEngine {
    /// 创建新的混合搜索引擎
    pub fn new(
        dense_engine: Arc<HnswVectorIndex>,
        sparse_engine: Arc<SparseIndex>,
        fusion_strategy: FusionStrategy,
    ) -> Self {
        Self {
            dense_engine,
            sparse_engine,
            tokenizer: SimpleTokenizer::new(),
            vocabulary: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            fusion_strategy,
            fusion_model: None,
            performance_stats: Arc::new(Mutex::new(HashMap::new())),
            query_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 创建带有学习式融合模型的搜索引擎
    pub fn with_fusion_model(
        dense_engine: Arc<HnswVectorIndex>,
        sparse_engine: Arc<SparseIndex>,
        fusion_strategy: FusionStrategy,
        fusion_model: Arc<Mutex<dyn FusionModel>>,
    ) -> Self {
        Self {
            dense_engine,
            sparse_engine,
            tokenizer: SimpleTokenizer::new(),
            vocabulary: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            fusion_strategy,
            fusion_model: Some(fusion_model),
            performance_stats: Arc::new(Mutex::new(HashMap::new())),
            query_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 添加文档到混合索引
    pub async fn add_document<S: VectorStore + ?Sized>(
        &self, 
        store: &S, 
        record: &DocumentRecord
    ) -> Result<()> {
        // 添加到密集向量索引
        let vector_point = crate::types::VectorPoint {
            vector: record.embedding.clone(),
            document_id: record.id.clone(),
        };
        self.dense_engine.add_point(vector_point)?;

        // 如果有稀疏向量表示，添加到稀疏索引
        if let Some(sparse_repr) = &record.sparse_representation {
            self.sparse_engine.add_document(sparse_repr)?;
        } else {
            // 如果没有预先计算的稀疏向量，现在计算
            let sparse_repr = self.compute_sparse_representation(&record.id, &record.content)?;
            self.sparse_engine.add_document(&sparse_repr)?;
        }

        Ok(())
    }

    /// 计算文档的稀疏向量表示
    fn compute_sparse_representation(&self, document_id: &str, content: &str) -> Result<crate::types::DocumentSparseRepresentation> {
        let vocabulary = self.vocabulary.read();
        
        // 如果词汇表为空，创建空的稀疏向量表示
        if vocabulary.is_empty() {
            drop(vocabulary);
            let empty_sparse_vector = crate::types::SparseVector::new(
                vec![], // 空索引
                vec![], // 空值
                0       // 空维度
            )?;
            return Ok(crate::types::DocumentSparseRepresentation {
                document_id: document_id.to_string(),
                sparse_vector: empty_sparse_vector,
                document_length: 0.0,
                term_frequencies: HashMap::new(),
            });
        }

        self.tokenizer.document_to_sparse_vector(document_id, content, &vocabulary)
    }

    /// 更新词汇表
    pub fn update_vocabulary(&self, documents: &[&str]) {
        let new_vocabulary = self.tokenizer.build_vocabulary(documents);
        let mut vocabulary = self.vocabulary.write();
        *vocabulary = new_vocabulary;
    }

    /// 混合搜索
    pub async fn search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        request: &HybridSearchRequest,
    ) -> Result<Vec<SearchResult>> {
        let mut search_results = Vec::new();

        // 1. 密集向量搜索
        let dense_results = if let Some(dense_vector) = &request.dense_vector {
            let results = self.dense_engine.search(dense_vector, request.limit * 2)?;
            results.into_iter()
                .map(|r| (r.document_id, r.similarity))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        // 2. 稀疏向量搜索（BM25）
        let sparse_results = if let Some(sparse_vector) = &request.sparse_vector {
            self.sparse_engine.search_bm25(sparse_vector, request.limit * 2)?
        } else if let Some(text_query) = &request.text_query {
            // 从文本查询构建稀疏向量
            let vocabulary = self.vocabulary.read();
            if !vocabulary.is_empty() {
                let sparse_repr = self.tokenizer.document_to_sparse_vector(
                    "query", 
                    text_query, 
                    &vocabulary
                )?;
                self.sparse_engine.search_bm25(&sparse_repr.sparse_vector, request.limit * 2)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // 3. 简单文本搜索（作为后备）
        let text_results = if let Some(text_query) = &request.text_query {
            self.simple_text_search(store, text_query, request.limit * 2).await?
        } else {
            Vec::new()
        };

        // 4. 融合结果
        let fused_results = self.fuse_results(
            &dense_results,
            &sparse_results, 
            &text_results,
            request,
        )?;

        // 5. 获取文档详情并构建最终结果
        for (doc_id, score, breakdown) in fused_results.into_iter().take(request.limit) {
            if let Some(doc) = store.get_document(&doc_id).await? {
                let result = SearchResult {
                    document_id: doc.id.clone(),
                    title: doc.title.clone(),
                    content_snippet: self.extract_snippet(&doc.content, request.text_query.as_deref()),
                    similarity_score: score,
                    package_name: doc.package_name.clone(),
                    doc_type: doc.doc_type.clone(),
                    metadata: doc.metadata,
                    score_breakdown: Some(breakdown),
                };
                search_results.push(result);
            }
        }

        Ok(search_results)
    }

    /// 融合多个搜索结果
    fn fuse_results(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        request: &HybridSearchRequest,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        match &self.fusion_strategy {
            FusionStrategy::RRF { k } => {
                self.rrf_fusion(dense_results, sparse_results, text_results, *k)
            }
            FusionStrategy::Linear { dense_weight, sparse_weight, text_weight } => {
                self.linear_fusion(
                    dense_results, 
                    sparse_results, 
                    text_results,
                    *dense_weight,
                    *sparse_weight, 
                    *text_weight
                )
            }
            FusionStrategy::Normalized { dense_weight, sparse_weight, text_weight } => {
                self.normalized_fusion(
                    dense_results,
                    sparse_results,
                    text_results,
                    *dense_weight,
                    *sparse_weight,
                    *text_weight
                )
            }
            FusionStrategy::Learned { base_weights, query_type_adaptation, quality_adaptation } => {
                self.learned_fusion(
                    dense_results,
                    sparse_results,
                    text_results,
                    base_weights,
                    *query_type_adaptation,
                    *quality_adaptation,
                    request
                )
            }
            FusionStrategy::Adaptive { initial_weights, learning_rate: _, history_size: _ } => {
                self.adaptive_fusion(
                    dense_results,
                    sparse_results,
                    text_results,
                    initial_weights,
                    request
                )
            }
        }
    }

    /// RRF (Reciprocal Rank Fusion) 融合算法
    fn rrf_fusion(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        k: f32,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        let mut document_scores: HashMap<String, (f32, ScoreBreakdown)> = HashMap::new();

        // 处理密集向量结果
        for (rank, (doc_id, similarity)) in dense_results.iter().enumerate() {
            let rrf_score = 1.0 / (k + (rank + 1) as f32);
            let breakdown = ScoreBreakdown {
                dense_score: Some(*similarity),
                sparse_score: None,
                text_score: None,
                final_score: rrf_score,
            };
            document_scores.insert(doc_id.clone(), (rrf_score, breakdown));
        }

        // 处理稀疏向量结果
        for (rank, (doc_id, bm25_score)) in sparse_results.iter().enumerate() {
            let rrf_score = 1.0 / (k + (rank + 1) as f32);
            if let Some((current_score, ref mut breakdown)) = document_scores.get_mut(doc_id) {
                *current_score += rrf_score;
                breakdown.sparse_score = Some(*bm25_score);
                breakdown.final_score = *current_score;
            } else {
                let breakdown = ScoreBreakdown {
                    dense_score: None,
                    sparse_score: Some(*bm25_score),
                    text_score: None,
                    final_score: rrf_score,
                };
                document_scores.insert(doc_id.clone(), (rrf_score, breakdown));
            }
        }

        // 处理文本搜索结果
        for (rank, (doc_id, text_score)) in text_results.iter().enumerate() {
            let rrf_score = 1.0 / (k + (rank + 1) as f32);
            if let Some((current_score, ref mut breakdown)) = document_scores.get_mut(doc_id) {
                *current_score += rrf_score;
                breakdown.text_score = Some(*text_score);
                breakdown.final_score = *current_score;
            } else {
                let breakdown = ScoreBreakdown {
                    dense_score: None,
                    sparse_score: None,
                    text_score: Some(*text_score),
                    final_score: rrf_score,
                };
                document_scores.insert(doc_id.clone(), (rrf_score, breakdown));
            }
        }

        // 按分数排序
        let mut results: Vec<_> = document_scores
            .into_iter()
            .map(|(doc_id, (score, breakdown))| (doc_id, score, breakdown))
            .collect();
        
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(results)
    }

    /// 线性权重融合
    fn linear_fusion(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        dense_weight: f32,
        sparse_weight: f32,
        text_weight: f32,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        let mut document_scores: HashMap<String, (f32, ScoreBreakdown)> = HashMap::new();

        // 处理密集向量结果
        for (doc_id, similarity) in dense_results {
            let weighted_score = similarity * dense_weight;
            let breakdown = ScoreBreakdown {
                dense_score: Some(*similarity),
                sparse_score: None,
                text_score: None,
                final_score: weighted_score,
            };
            document_scores.insert(doc_id.clone(), (weighted_score, breakdown));
        }

        // 处理稀疏向量结果
        for (doc_id, bm25_score) in sparse_results {
            let weighted_score = bm25_score * sparse_weight;
            if let Some((current_score, ref mut breakdown)) = document_scores.get_mut(doc_id) {
                *current_score += weighted_score;
                breakdown.sparse_score = Some(*bm25_score);
                breakdown.final_score = *current_score;
            } else {
                let breakdown = ScoreBreakdown {
                    dense_score: None,
                    sparse_score: Some(*bm25_score),
                    text_score: None,
                    final_score: weighted_score,
                };
                document_scores.insert(doc_id.clone(), (weighted_score, breakdown));
            }
        }

        // 处理文本搜索结果
        for (doc_id, text_score) in text_results {
            let weighted_score = text_score * text_weight;
            if let Some((current_score, ref mut breakdown)) = document_scores.get_mut(doc_id) {
                *current_score += weighted_score;
                breakdown.text_score = Some(*text_score);
                breakdown.final_score = *current_score;
            } else {
                let breakdown = ScoreBreakdown {
                    dense_score: None,
                    sparse_score: None,
                    text_score: Some(*text_score),
                    final_score: weighted_score,
                };
                document_scores.insert(doc_id.clone(), (weighted_score, breakdown));
            }
        }

        // 按分数排序
        let mut results: Vec<_> = document_scores
            .into_iter()
            .map(|(doc_id, (score, breakdown))| (doc_id, score, breakdown))
            .collect();
        
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(results)
    }

    /// 标准化分数融合
    fn normalized_fusion(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        dense_weight: f32,
        sparse_weight: f32,
        text_weight: f32,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        // 标准化各类型的分数到 [0, 1] 范围
        let normalized_dense = self.normalize_scores(dense_results);
        let normalized_sparse = self.normalize_scores(sparse_results);
        let normalized_text = self.normalize_scores(text_results);

        // 使用标准化后的分数进行线性融合
        self.linear_fusion(
            &normalized_dense,
            &normalized_sparse,
            &normalized_text,
            dense_weight,
            sparse_weight,
            text_weight,
        )
    }

    /// 分数标准化
    fn normalize_scores(&self, results: &[(String, f32)]) -> Vec<(String, f32)> {
        if results.is_empty() {
            return Vec::new();
        }

        let max_score = results.iter()
            .map(|(_, score)| *score)
            .fold(f32::NEG_INFINITY, f32::max);
        
        let min_score = results.iter()
            .map(|(_, score)| *score)
            .fold(f32::INFINITY, f32::min);

        let score_range = max_score - min_score;

        results.iter()
            .map(|(doc_id, score)| {
                let normalized_score = if score_range > 0.0 {
                    (score - min_score) / score_range
                } else {
                    1.0 // 如果所有分数相同，设为1.0
                };
                (doc_id.clone(), normalized_score)
            })
            .collect()
    }

    /// 简单文本搜索（后备方案）
    async fn simple_text_search<S: VectorStore + ?Sized>(
        &self,
        store: &S,
        query_text: &str,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let text_lower = query_text.to_lowercase();
        
        // 使用分页方式处理大量文档，避免一次性加载过多数据
        let mut all_results = Vec::new();
        let page_size = 500; // 每次处理500个文档
        let mut offset = 0;
        let max_docs = 10000; // 最多处理10000个文档
        
        while offset < max_docs {
            let docs = store.list_documents(offset, page_size).await?;
            
            if docs.is_empty() {
                break; // 没有更多文档了
            }
            
            for doc in docs {
                let content_lower = doc.content.to_lowercase();
                let title_lower = doc.title.to_lowercase();
                
                let mut score = 0.0;
                
                // 计算文本匹配分数
                for term in text_lower.split_whitespace() {
                    if content_lower.contains(term) {
                        score += 1.0;
                    }
                    if title_lower.contains(term) {
                        score += 2.0; // 标题匹配权重更高
                    }
                }
                
                if score > 0.0 {
                    all_results.push((doc.id, score));
                }
            }
            
            offset += page_size;
        }
        
        // 按分数排序并限制结果数量
        all_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(limit);
        
        Ok(all_results)
    }

    /// 提取内容摘要
    fn extract_snippet(&self, content: &str, query_text: Option<&str>) -> String {
        let max_length = 200;
        
        if let Some(query) = query_text {
            let query_lower = query.to_lowercase();
            let content_lower = content.to_lowercase();
            
            if let Some(pos) = content_lower.find(&query_lower) {
                let start = pos.saturating_sub(50);
                let end = (pos + query.len() + 150).min(content.len());
                let snippet = &content[start..end];
                
                if snippet.len() > max_length {
                    format!("...{}", &snippet[..max_length])
                } else if start > 0 {
                    format!("...{}", snippet)
                } else {
                    snippet.to_string()
                }
            } else {
                content.chars().take(max_length).collect()
            }
        } else {
            content.chars().take(max_length).collect()
        }
    }

    /// 删除文档
    pub async fn remove_document(&self, document_id: &str) -> Result<bool> {
        let dense_removed = self.dense_engine.remove_point(document_id)?;
        let sparse_removed = self.sparse_engine.remove_document(document_id)?;
        
        Ok(dense_removed || sparse_removed)
    }

    /// 学习式融合算法
    fn learned_fusion(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        base_weights: &FusionWeights,
        query_type_adaptation: bool,
        quality_adaptation: bool,
        request: &HybridSearchRequest,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        let weights = if query_type_adaptation {
            // 根据查询特征调整权重
            let context = self.analyze_query_context(request);
            if let Some(model) = &self.fusion_model {
                let model_guard = model.lock()
                    .map_err(|_| VectorDbError::Other("Failed to acquire model lock".to_string()))?;
                model_guard.predict_weights(
                    request.text_query.as_deref().unwrap_or(""),
                    &context
                )
            } else {
                base_weights.clone()
            }
        } else {
            base_weights.clone()
        };

        // 应用质量调整
        let adjusted_weights = if quality_adaptation {
            self.adjust_weights_by_quality(&weights, dense_results, sparse_results, text_results)
        } else {
            weights
        };

        // 使用调整后的权重进行线性融合
        self.linear_fusion(
            dense_results,
            sparse_results,
            text_results,
            adjusted_weights.dense_weight,
            adjusted_weights.sparse_weight,
            adjusted_weights.text_weight,
        )
    }

    /// 自适应融合算法
    fn adaptive_fusion(
        &self,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
        initial_weights: &FusionWeights,
        request: &HybridSearchRequest,
    ) -> Result<Vec<(String, f32, ScoreBreakdown)>> {
        // 分析历史查询性能，调整权重
        let adjusted_weights = self.adapt_weights_from_history(initial_weights, request)?;

        // 使用调整后的权重进行线性融合
        self.linear_fusion(
            dense_results,
            sparse_results,
            text_results,
            adjusted_weights.dense_weight,
            adjusted_weights.sparse_weight,
            adjusted_weights.text_weight,
        )
    }

    /// 分析查询上下文
    fn analyze_query_context(&self, request: &HybridSearchRequest) -> FusionContext {
        let query_text = request.text_query.as_deref().unwrap_or("");
        
        // 简单的查询类型分析
        let query_type = if query_text.len() > 20 && query_text.contains(' ') {
            QueryType::Semantic  // 长句子，语义查询
        } else if query_text.len() <= 5 || !query_text.contains(' ') {
            QueryType::Keyword   // 短词，关键词查询
        } else {
            QueryType::Mixed     // 混合查询
        };

        // 简单的时间上下文（可以扩展为更复杂的逻辑）
        let time_context = TimeContext::WorkingHours;

        FusionContext {
            query_type,
            query_length: query_text.len(),
            historical_ctr: 0.5, // 默认CTR
            time_context,
        }
    }

    /// 根据结果质量调整权重
    fn adjust_weights_by_quality(
        &self,
        base_weights: &FusionWeights,
        dense_results: &[(String, f32)],
        sparse_results: &[(String, f32)],
        text_results: &[(String, f32)],
    ) -> FusionWeights {
        // 计算各搜索策略的结果质量指标
        let dense_quality = self.calculate_result_quality(dense_results);
        let sparse_quality = self.calculate_result_quality(sparse_results);
        let text_quality = self.calculate_result_quality(text_results);

        // 根据质量调整权重
        let total_quality = dense_quality + sparse_quality + text_quality;
        if total_quality > 0.0 {
            FusionWeights {
                dense_weight: base_weights.dense_weight * (1.0 + dense_quality / total_quality * 0.2),
                sparse_weight: base_weights.sparse_weight * (1.0 + sparse_quality / total_quality * 0.2),
                text_weight: base_weights.text_weight * (1.0 + text_quality / total_quality * 0.2),
            }
        } else {
            base_weights.clone()
        }
    }

    /// 计算结果质量
    fn calculate_result_quality(&self, results: &[(String, f32)]) -> f32 {
        if results.is_empty() {
            return 0.0;
        }

        // 简单的质量指标：结果数量 + 平均分数 + 分数分布
        let count_factor = (results.len() as f32).min(10.0) / 10.0;
        let avg_score = results.iter().map(|(_, score)| score).sum::<f32>() / results.len() as f32;
        let score_variance = self.calculate_score_variance(results);
        
        count_factor * 0.3 + avg_score * 0.5 + (1.0 - score_variance).max(0.0) * 0.2
    }

    /// 计算分数方差
    fn calculate_score_variance(&self, results: &[(String, f32)]) -> f32 {
        if results.len() < 2 {
            return 0.0;
        }

        let avg = results.iter().map(|(_, score)| score).sum::<f32>() / results.len() as f32;
        let variance = results.iter()
            .map(|(_, score)| (score - avg).powi(2))
            .sum::<f32>() / results.len() as f32;
        
        variance.sqrt()
    }

    /// 基于历史记录调整权重
    fn adapt_weights_from_history(
        &self,
        initial_weights: &FusionWeights,
        request: &HybridSearchRequest,
    ) -> Result<FusionWeights> {
        // 获取相似查询的历史性能
        let history = self.query_history.lock()
            .map_err(|_| VectorDbError::Other("Failed to acquire query history lock".to_string()))?;
        let query_text = request.text_query.as_deref().unwrap_or("");
        
        // 找到相似的历史查询
        let similar_queries: Vec<_> = history.iter()
            .filter(|metrics| {
                self.calculate_query_similarity(&metrics.query_text, query_text) > 0.7
            })
            .collect();

        if similar_queries.is_empty() {
            return Ok(initial_weights.clone());
        }

        // 基于相似查询的性能调整权重
        let mut adjusted_weights = initial_weights.clone();
        
        let avg_satisfaction = similar_queries.iter()
            .filter_map(|q| q.user_satisfaction)
            .map(|s| s as f32 / 5.0)
            .sum::<f32>() / similar_queries.len() as f32;

        // 如果历史满意度低，增加探索性（减少主导策略的权重）
        if avg_satisfaction < 0.6 {
            adjusted_weights.dense_weight *= 0.9;
            adjusted_weights.sparse_weight *= 1.1;
            adjusted_weights.text_weight *= 1.05;
        }

        Ok(adjusted_weights)
    }

    /// 计算查询相似度
    fn calculate_query_similarity(&self, query1: &str, query2: &str) -> f32 {
        // 简单的词汇重叠相似度
        let words1: std::collections::HashSet<&str> = query1.split_whitespace().collect();
        let words2: std::collections::HashSet<&str> = query2.split_whitespace().collect();
        
        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();
        
        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    /// 记录查询指标（用于学习）
    pub fn record_query_metrics(&self, metrics: QueryMetrics) -> Result<()> {
        // 更新历史记录
        {
            let mut history = self.query_history.lock().unwrap();
            history.push(metrics.clone());
            
            // 限制历史记录大小
            if history.len() > 10000 {
                history.remove(0);
            }
        }

        // 如果有学习模型，更新模型
        if let Some(model) = &self.fusion_model {
            let mut model_guard = model.lock().unwrap();
            model_guard.update_model(&metrics)?;
        }

        Ok(())
    }

    /// 获取性能统计
    pub fn get_performance_stats(&self) -> HashMap<String, FusionPerformanceStats> {
        self.performance_stats.lock().unwrap().clone()
    }

    /// 计算缓存命中率
    fn calculate_cache_hit_rate(&self) -> f64 {
        // 基于查询历史计算缓存命中率
        let history = self.query_history.lock().unwrap();
        if history.is_empty() {
            return 0.0;
        }
        
        let total_queries = history.len() as f64;
        let cache_hits = history.iter()
            .filter(|metrics| metrics.duration_ms < 10.0) // 假设小于10ms的查询为缓存命中
            .count() as f64;
            
        if total_queries > 0.0 {
            cache_hits / total_queries
        } else {
            0.0
        }
    }

    /// 获取搜索引擎统计信息
    pub fn get_stats(&self) -> crate::types::DatabaseStats {
        let sparse_stats = self.sparse_engine.get_stats();
        let dense_stats = self.dense_engine.get_stats();
        
        crate::types::DatabaseStats {
            document_count: sparse_stats.total_documents,
            dense_vector_count: dense_stats.point_count,
            sparse_vector_count: sparse_stats.total_documents,
            memory_usage_mb: dense_stats.memory_usage_mb + self.sparse_engine.get_memory_usage_mb(),
            dense_index_size_mb: dense_stats.memory_usage_mb,
            sparse_index_size_mb: self.sparse_engine.get_memory_usage_mb(),
            cache_hit_rate: self.calculate_cache_hit_rate(),
            bm25_stats: Some(sparse_stats),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        sparse::{SparseIndex, BM25Parameters},
        index::{HnswVectorIndex},
        config::HnswConfig,
        types::FusionStrategy,
    };

    #[test]
    fn test_rrf_fusion() {
        let dense_engine = Arc::new(HnswVectorIndex::new());
        let sparse_engine = Arc::new(SparseIndex::new(BM25Parameters::default()));
        
        let engine = HybridSearchEngine::new(
            dense_engine,
            sparse_engine,
            FusionStrategy::RRF { k: 60.0 },
        );

        let dense_results = vec![
            ("doc1".to_string(), 0.9),
            ("doc2".to_string(), 0.8),
        ];
        
        let sparse_results = vec![
            ("doc2".to_string(), 5.0),
            ("doc3".to_string(), 4.0),
        ];
        
        let text_results = vec![
            ("doc1".to_string(), 2.0),
            ("doc4".to_string(), 1.0),
        ];

        let fused = engine.rrf_fusion(&dense_results, &sparse_results, &text_results, 60.0).unwrap();
        
        // 检查结果
        assert!(!fused.is_empty());
        
        // doc1和doc2应该有更高的分数（出现在多个结果中）
        let doc1_score = fused.iter().find(|(id, _, _)| id == "doc1").map(|(_, score, _)| *score);
        let doc3_score = fused.iter().find(|(id, _, _)| id == "doc3").map(|(_, score, _)| *score);
        
        if let (Some(doc1), Some(doc3)) = (doc1_score, doc3_score) {
            assert!(doc1 > doc3, "doc1 should have higher score than doc3");
        }
    }
} 