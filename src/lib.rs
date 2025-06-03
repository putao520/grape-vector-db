//! # Grape Vector Database
//!
//! 一个高性能的嵌入式向量数据库，专为AI应用和语义搜索设计。
//!
//! ## 特性
//!
//! - **高性能**: 基于HNSW算法的近似最近邻搜索
//! - **嵌入式**: 无需外部服务，直接集成到应用中
//! - **智能缓存**: 多层缓存策略，减少API调用70%
//! - **混合搜索**: 结合向量相似度和文本匹配
//! - **持久化**: 支持磁盘存储和数据恢复
//! - **批量操作**: 高效的批量插入和查询
//! - **去重**: 智能的重复文档检测
//! - **多提供商**: 支持OpenAI、Azure、Ollama等嵌入服务
//!
//! ## 快速开始
//!
//! ### 使用Mock提供商（测试）
//! ```rust
//! use grape_vector_db::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建向量数据库实例（默认使用mock提供商）
//!     let mut db = VectorDatabase::new("./data").await?;
//!     
//!     // 添加文档
//!     let doc = Document {
//!         id: "doc1".to_string(),
//!         content: "Rust是一种系统编程语言".to_string(),
//!         title: Some("Rust介绍".to_string()),
//!         language: Some("zh".to_string()),
//!         ..Default::default()
//!     };
//!     
//!     db.add_document(doc).await?;
//!     
//!     // 搜索相似文档
//!     let results = db.search("编程语言", 10).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### 使用OpenAI API
//! ```rust
//! use grape_vector_db::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 使用OpenAI API创建向量数据库
//!     let mut db = VectorDatabase::with_openai_compatible(
//!         "./data",
//!         "https://api.openai.com/v1/embeddings".to_string(),
//!         "your-api-key".to_string(),
//!         "text-embedding-3-small".to_string()
//!     ).await?;
//!     
//!     // 其余使用方式相同...
//!     Ok(())
//! }
//! ```
//!
//! ### 使用Azure OpenAI
//! ```rust
//! use grape_vector_db::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 使用Azure OpenAI创建向量数据库
//!     let mut db = VectorDatabase::with_azure_openai(
//!         "./data",
//!         "https://your-resource.openai.azure.com".to_string(),
//!         "your-api-key".to_string(),
//!         "your-deployment-name".to_string(),
//!         Some("2023-05-15".to_string())
//!     ).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### 使用本地Ollama
//! ```rust
//! use grape_vector_db::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 使用本地Ollama创建向量数据库
//!     let mut db = VectorDatabase::with_ollama(
//!         "./data",
//!         Some("http://localhost:11434".to_string()),
//!         "nomic-embed-text".to_string()
//!     ).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### 使用自定义配置
//! ```rust
//! use grape_vector_db::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut config = VectorDbConfig::default();
//!     config.embedding.provider = "openai".to_string();
//!     config.embedding.endpoint = Some("https://api.openai.com/v1/embeddings".to_string());
//!     config.embedding.api_key = Some("your-api-key".to_string());
//!     config.embedding.model = "text-embedding-3-large".to_string();
//!     config.embedding.dimension = Some(3072); // text-embedding-3-large 的维度
//!     
//!     let mut db = VectorDatabase::with_config("./data", config).await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod types;
pub mod config;
pub mod storage;
pub mod embeddings;
pub mod query;
pub mod index;
pub mod metrics;
pub mod errors;

// 主要组件的重新导出
pub use types::{Document, DocumentRecord, SearchResult, VectorPoint, DatabaseStats};
pub use config::VectorDbConfig;
pub use storage::{VectorStore, BasicVectorStore};
pub use embeddings::{
    EmbeddingProvider, 
    MockEmbeddingProvider, 
    OpenAICompatibleProvider,
    create_provider,
    create_openai_compatible_provider
};
pub use query::QueryEngine;
pub use index::HnswIndex;
pub use metrics::{PerformanceMetrics, MetricsCollector};
pub use errors::{VectorDbError, Result};

use std::path::Path;

/// 主要的向量数据库接口
pub struct VectorDatabase {
    store: Box<dyn VectorStore>,
    embedding_provider: Box<dyn EmbeddingProvider>,
    query_engine: QueryEngine,
    config: VectorDbConfig,
}

impl VectorDatabase {
    /// 创建新的向量数据库实例
    pub async fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let config = VectorDbConfig::default();
        Self::with_config(data_dir, config).await
    }

    /// 使用自定义配置创建向量数据库实例
    pub async fn with_config<P: AsRef<Path>>(data_dir: P, config: VectorDbConfig) -> Result<Self> {
        let store = Box::new(BasicVectorStore::new(data_dir.as_ref().to_path_buf(), &config).await?);
        let embedding_provider = create_provider(&config)?;
        let query_engine = QueryEngine::new(&config);

        Ok(Self {
            store,
            embedding_provider,
            query_engine,
            config,
        })
    }

    /// 便捷方法：使用OpenAI兼容API创建向量数据库
    /// 
    /// # 参数
    /// - `data_dir`: 数据存储目录
    /// - `endpoint`: 嵌入API端点URL
    /// - `api_key`: API密钥或token
    /// - `model`: 嵌入模型名称
    /// 
    /// # 示例
    /// ```rust
    /// let db = VectorDatabase::with_openai_compatible(
    ///     "./data",
    ///     "https://api.openai.com/v1/embeddings",
    ///     "your-api-key",
    ///     "text-embedding-3-small"
    /// ).await?;
    /// ```
    pub async fn with_openai_compatible<P: AsRef<Path>>(
        data_dir: P,
        endpoint: String,
        api_key: String,
        model: String,
    ) -> Result<Self> {
        let mut config = VectorDbConfig::default();
        config.embedding.provider = "openai".to_string();
        config.embedding.endpoint = Some(endpoint);
        config.embedding.api_key = Some(api_key);
        config.embedding.model = model;
        
        Self::with_config(data_dir, config).await
    }

    /// 便捷方法：使用Azure OpenAI创建向量数据库
    /// 
    /// # 参数
    /// - `data_dir`: 数据存储目录
    /// - `endpoint`: Azure OpenAI端点URL (例如: https://your-resource.openai.azure.com)
    /// - `api_key`: Azure API密钥
    /// - `deployment_name`: 部署名称
    /// - `api_version`: API版本 (可选，默认为"2023-05-15")
    pub async fn with_azure_openai<P: AsRef<Path>>(
        data_dir: P,
        endpoint: String,
        api_key: String,
        deployment_name: String,
        api_version: Option<String>,
    ) -> Result<Self> {
        let mut config = VectorDbConfig::default();
        config.embedding.provider = "azure".to_string();
        config.embedding.endpoint = Some(endpoint);
        config.embedding.api_key = Some(api_key);
        config.embedding.model = deployment_name;
        config.embedding.api_version = api_version;
        
        Self::with_config(data_dir, config).await
    }

    /// 便捷方法：使用本地Ollama创建向量数据库
    /// 
    /// # 参数
    /// - `data_dir`: 数据存储目录
    /// - `ollama_url`: Ollama服务URL (可选，默认为http://localhost:11434)
    /// - `model`: 嵌入模型名称 (例如: "nomic-embed-text")
    pub async fn with_ollama<P: AsRef<Path>>(
        data_dir: P,
        ollama_url: Option<String>,
        model: String,
    ) -> Result<Self> {
        let mut config = VectorDbConfig::default();
        config.embedding.provider = "ollama".to_string();
        config.embedding.endpoint = Some(ollama_url.unwrap_or_else(|| "http://localhost:11434/api/embeddings".to_string()));
        config.embedding.model = model;
        config.embedding.api_key = None; // Ollama通常不需要API密钥
        
        Self::with_config(data_dir, config).await
    }

    /// 添加单个文档
    pub async fn add_document(&mut self, document: Document) -> Result<String> {
        let embedding = self.embedding_provider.generate_embedding(&document.content).await?;
        
        let record = DocumentRecord {
            id: document.id.clone(),
            content: document.content,
            title: document.title.unwrap_or_else(|| "未标题".to_string()),
            language: document.language.unwrap_or_else(|| "unknown".to_string()),
            package_name: document.package_name.unwrap_or_else(|| "unknown".to_string()),
            version: document.version.unwrap_or_else(|| "latest".to_string()),
            doc_type: document.doc_type.unwrap_or_else(|| "general".to_string()),
            metadata: document.metadata,
            embedding,
        };

        self.store.add_document(record).await?;
        Ok(document.id)
    }

    /// 批量添加文档
    pub async fn add_documents(&mut self, documents: Vec<Document>) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for document in documents {
            let id = self.add_document(document).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// 搜索相似文档
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let query_vector = self.embedding_provider.generate_embedding(query).await?;
        self.query_engine.search(self.store.as_ref(), &query_vector, query, limit).await
    }

    /// 获取数据库统计信息
    pub fn stats(&self) -> DatabaseStats {
        self.store.stats()
    }

    /// 保存数据到磁盘
    pub async fn save(&self) -> Result<()> {
        self.store.save().await
    }

    /// 从磁盘加载数据
    pub async fn load(&mut self) -> Result<()> {
        self.store.load().await
    }
}
