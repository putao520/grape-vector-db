use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 文档结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 文档唯一标识符
    pub id: String,
    /// 文档内容
    pub content: String,
    /// 文档标题（可选）
    pub title: Option<String>,
    /// 语言代码（可选）
    pub language: Option<String>,
    /// 包名（可选）
    pub package_name: Option<String>,
    /// 版本号（可选）
    pub version: Option<String>,
    /// 文档类型（可选）
    pub doc_type: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            id: String::new(),
            content: String::new(),
            title: None,
            language: None,
            package_name: None,
            version: None,
            doc_type: None,
            metadata: HashMap::new(),
        }
    }
}

/// 文档记录（内部存储格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    /// 文档唯一标识符
    pub id: String,
    /// 文档内容
    pub content: String,
    /// 文档标题
    pub title: String,
    /// 语言代码
    pub language: String,
    /// 包名
    pub package_name: String,
    /// 版本号
    pub version: String,
    /// 文档类型
    pub doc_type: String,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 嵌入向量
    pub embedding: Vec<f32>,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 文档ID
    pub document_id: String,
    /// 文档标题
    pub title: String,
    /// 文档内容片段
    pub content_snippet: String,
    /// 相似度分数 (0.0 - 1.0)
    pub similarity_score: f32,
    /// 匹配的包名
    pub package_name: String,
    /// 文档类型
    pub doc_type: String,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 向量点（用于HNSW索引）
#[derive(Debug, Clone)]
pub struct VectorPoint {
    /// 向量数据
    pub vector: Vec<f32>,
    /// 关联的文档ID
    pub document_id: String,
}

/// 数据库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// 文档总数
    pub document_count: usize,
    /// 向量总数
    pub vector_count: usize,
    /// 内存使用量（MB）
    pub memory_usage_mb: f64,
    /// 索引大小（MB）
    pub index_size_mb: f64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
}

impl Default for DatabaseStats {
    fn default() -> Self {
        Self {
            document_count: 0,
            vector_count: 0,
            memory_usage_mb: 0.0,
            index_size_mb: 0.0,
            cache_hit_rate: 0.0,
        }
    }
} 