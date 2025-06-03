//! 向量数据库错误处理模块
//! 
//! 定义向量数据库操作中可能出现的各种错误类型

use std::fmt;

/// 向量数据库操作结果类型
pub type Result<T> = std::result::Result<T, VectorDbError>;

/// 向量数据库错误类型
#[derive(Debug)]
pub enum VectorDbError {
    /// I/O错误
    Io(std::io::Error),
    /// 序列化/反序列化错误
    Serde(serde_json::Error),
    /// 配置错误
    Config(String),
    /// 存储错误
    Storage(String),
    /// 索引错误
    Index(String),
    /// 嵌入生成错误
    Embedding(String),
    /// 查询错误
    Query(String),
    /// 文档未找到
    DocumentNotFound(String),
    /// 重复文档
    DuplicateDocument(String),
    /// 向量维度不匹配
    InvalidVectorDimension { expected: usize, actual: usize },
    /// 缓存错误
    Cache(String),
    /// 通用错误
    Other(String),
}

impl fmt::Display for VectorDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorDbError::Io(err) => write!(f, "I/O错误: {}", err),
            VectorDbError::Serde(err) => write!(f, "序列化错误: {}", err),
            VectorDbError::Config(msg) => write!(f, "配置错误: {}", msg),
            VectorDbError::Storage(msg) => write!(f, "存储错误: {}", msg),
            VectorDbError::Index(msg) => write!(f, "索引错误: {}", msg),
            VectorDbError::Embedding(msg) => write!(f, "嵌入生成错误: {}", msg),
            VectorDbError::Query(msg) => write!(f, "查询错误: {}", msg),
            VectorDbError::DocumentNotFound(id) => write!(f, "文档未找到: {}", id),
            VectorDbError::DuplicateDocument(id) => write!(f, "重复文档: {}", id),
            VectorDbError::InvalidVectorDimension { expected, actual } => {
                write!(f, "向量维度不匹配，期望: {}, 实际: {}", expected, actual)
            }
            VectorDbError::Cache(msg) => write!(f, "缓存错误: {}", msg),
            VectorDbError::Other(msg) => write!(f, "错误: {}", msg),
        }
    }
}

impl std::error::Error for VectorDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VectorDbError::Io(err) => Some(err),
            VectorDbError::Serde(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VectorDbError {
    fn from(err: std::io::Error) -> Self {
        VectorDbError::Io(err)
    }
}

impl From<serde_json::Error> for VectorDbError {
    fn from(err: serde_json::Error) -> Self {
        VectorDbError::Serde(err)
    }
}

impl VectorDbError {
    /// 创建配置错误
    pub fn config_error(msg: impl Into<String>) -> Self {
        VectorDbError::Config(msg.into())
    }

    /// 创建索引错误
    pub fn index_error(msg: impl Into<String>) -> Self {
        VectorDbError::Index(msg.into())
    }

    /// 创建嵌入生成错误
    pub fn embedding_error(msg: impl Into<String>) -> Self {
        VectorDbError::Embedding(msg.into())
    }

    /// 创建查询错误
    pub fn query_error(msg: impl Into<String>) -> Self {
        VectorDbError::Query(msg.into())
    }

    /// 创建存储错误
    pub fn storage_error(msg: impl Into<String>) -> Self {
        VectorDbError::Storage(msg.into())
    }

    /// 创建缓存错误
    pub fn cache_error(msg: impl Into<String>) -> Self {
        VectorDbError::Cache(msg.into())
    }

    /// 创建通用错误
    pub fn other(msg: impl Into<String>) -> Self {
        VectorDbError::Other(msg.into())
    }
} 