use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

static SYSTEM_CONFIG: OnceLock<SystemConfig> = OnceLock::new();

/// 系统配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub vector_search: VectorSearchConfig,
    pub api_limits: ApiLimitsConfig,
    pub content_analysis: ContentAnalysisConfig,
    pub similarity_detection: SimilarityDetectionConfig,
    pub performance: PerformanceConfig,
    pub ai_integration: AiIntegrationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchConfig {
    pub cache_limit: usize,
    pub similarity_threshold: f32,
    pub search_timeout_ms: u64,
    pub max_results_per_query: usize,
    pub embedding_cache_ttl_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiLimitsConfig {
    pub github_per_page: usize,
    pub batch_processing_size: usize,
    pub concurrent_requests: usize,
    pub retry_attempts: u32,
    pub backoff_multiplier: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAnalysisConfig {
    pub min_content_length: usize,
    pub max_document_length: usize,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub quality_weights: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityDetectionConfig {
    pub text_similarity_weight: f32,
    pub structure_similarity_weight: f32,
    pub keyword_similarity_weight: f32,
    pub complexity_similarity_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub cache_cleanup_interval_hours: u64,
    pub vector_dimension: usize,
    pub index_rebuild_threshold: usize,
    pub memory_limit_mb: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIntegrationConfig {
    pub default_model: String,
    pub fallback_to_statistical: bool,
    pub enable_real_ai_analysis: bool,
    pub api_timeout_seconds: u64,
}

/// 向量数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbConfig {
    /// 向量维度
    pub vector_dimension: usize,
    
    /// HNSW 配置
    pub hnsw: HnswConfig,
    
    /// 嵌入提供者配置
    pub embedding: EmbeddingConfig,
    
    /// 缓存配置
    pub cache: CacheConfig,
    
    /// 持久化配置
    pub persistence: PersistenceConfig,
    
    /// 查询配置
    pub query: QueryConfig,
}

/// HNSW 索引配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// 构建时的连接数
    pub m: usize,
    
    /// 构建时的候选数
    pub ef_construction: usize,
    
    /// 搜索时的候选数
    pub ef_search: usize,
    
    /// 最大层数
    pub max_layers: usize,
}

/// 嵌入配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// 提供者类型 (openai, azure, nvidia, huggingface, local, mock)
    pub provider: String,
    
    /// API 端点URL (支持自定义端点)
    pub endpoint: Option<String>,
    
    /// API 密钥或token
    pub api_key: Option<String>,
    
    /// 模型名称
    pub model: String,
    
    /// API 版本（用于Azure等服务）
    pub api_version: Option<String>,
    
    /// 自定义请求头
    pub headers: HashMap<String, String>,
    
    /// 批量大小
    pub batch_size: usize,
    
    /// 请求超时（秒）
    pub timeout_seconds: u64,
    
    /// 重试次数
    pub retry_attempts: u32,
    
    /// 向量维度（如果已知）
    pub dimension: Option<usize>,
}

/// 缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// 嵌入缓存大小
    pub embedding_cache_size: usize,
    
    /// 查询结果缓存大小
    pub query_cache_size: usize,
    
    /// 缓存TTL（秒）
    pub cache_ttl_seconds: u64,
}

/// 持久化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    /// 数据目录
    pub data_dir: String,
    
    /// 自动保存间隔（秒）
    pub auto_save_interval_seconds: u64,
    
    /// 压缩选项
    pub compression: bool,
    
    /// 备份选项
    pub backup: bool,
}

/// 查询配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    /// 默认搜索限制
    pub default_limit: usize,
    
    /// 最大搜索限制
    pub max_limit: usize,
    
    /// 混合搜索权重
    pub hybrid_weights: HybridWeights,
    
    /// 相似度阈值
    pub similarity_threshold: f32,
}

/// 混合搜索权重
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridWeights {
    /// 向量相似度权重
    pub vector: f32,
    
    /// 关键词匹配权重
    pub keyword: f32,
    
    /// 上下文权重
    pub context: f32,
}

impl Default for SystemConfig {
    fn default() -> Self {
        let mut quality_weights = HashMap::new();
        quality_weights.insert("word_count".to_string(), 0.15);
        quality_weights.insert("sentence_count".to_string(), 0.10);
        quality_weights.insert("avg_word_length".to_string(), 0.10);
        quality_weights.insert("code_block_count".to_string(), 0.20);
        quality_weights.insert("link_count".to_string(), 0.15);
        quality_weights.insert("heading_count".to_string(), 0.15);
        quality_weights.insert("complexity_score".to_string(), 0.15);

        Self {
            vector_search: VectorSearchConfig {
                cache_limit: 1000,
                similarity_threshold: 0.85,
                search_timeout_ms: 1000,
                max_results_per_query: 100,
                embedding_cache_ttl_hours: 24,
            },
            api_limits: ApiLimitsConfig {
                github_per_page: 100,
                batch_processing_size: 50,
                concurrent_requests: 10,
                retry_attempts: 3,
                backoff_multiplier: 2,
            },
            content_analysis: ContentAnalysisConfig {
                min_content_length: 100,
                max_document_length: 10000,
                chunk_size: 1000,
                chunk_overlap: 100,
                quality_weights,
            },
            similarity_detection: SimilarityDetectionConfig {
                text_similarity_weight: 0.4,
                structure_similarity_weight: 0.25,
                keyword_similarity_weight: 0.25,
                complexity_similarity_weight: 0.1,
            },
            performance: PerformanceConfig {
                cache_cleanup_interval_hours: 12,
                vector_dimension: 1536,
                index_rebuild_threshold: 10000,
                memory_limit_mb: 512,
            },
            ai_integration: AiIntegrationConfig {
                default_model: "nvidia/nv-embedqa-mistral-7b-v2".to_string(),
                fallback_to_statistical: true,
                enable_real_ai_analysis: true,
                api_timeout_seconds: 30,
            },
        }
    }
}

impl SystemConfig {
    /// 加载配置文件
    pub fn load() -> &'static SystemConfig {
        SYSTEM_CONFIG.get_or_init(|| {
            // 尝试从多个位置加载配置文件
            let config_paths = vec![
                "config/system_config.toml",
                "system_config.toml",
                "./config/system_config.toml",
            ];
            
            for path in config_paths {
                if Path::new(path).exists() {
                    match Self::load_from_file(path) {
                        Ok(config) => {
                            tracing::info!("✅ 已加载配置文件: {}", path);
                            return config;
                        }
                        Err(e) => {
                            tracing::warn!("⚠️ 无法加载配置文件 {}: {}", path, e);
                        }
                    }
                }
            }
            
            tracing::info!("📝 使用默认配置");
            Self::default()
        })
    }
    
    /// 从文件加载配置
    fn load_from_file(path: &str) -> Result<SystemConfig> {
        let content = std::fs::read_to_string(path)?;
        let config: SystemConfig = toml::from_str(&content)?;
        Ok(config)
    }
    
    /// 获取全局配置实例
    pub fn get() -> &'static SystemConfig {
        SYSTEM_CONFIG.get().unwrap_or_else(|| {
            // 如果配置还没有初始化，先初始化它
            Self::load()
        })
    }
    
    /// 保存配置到文件
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        tracing::info!("💾 配置已保存到: {}", path);
        Ok(())
    }
}

impl Default for VectorDbConfig {
    fn default() -> Self {
        Self {
            vector_dimension: 768,
            hnsw: HnswConfig::default(),
            embedding: EmbeddingConfig::default(),
            cache: CacheConfig::default(),
            persistence: PersistenceConfig::default(),
            query: QueryConfig::default(),
        }
    }
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            ef_search: 100,
            max_layers: 16,
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "mock".to_string(), // 默认使用mock以便于测试
            endpoint: None,
            api_key: None,
            model: "text-embedding-3-small".to_string(), // OpenAI默认模型
            api_version: None,
            headers: HashMap::new(),
            batch_size: 32,
            timeout_seconds: 30,
            retry_attempts: 3,
            dimension: Some(1536), // OpenAI text-embedding-3-small 的维度
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            embedding_cache_size: 10000,
            query_cache_size: 1000,
            cache_ttl_seconds: 86400, // 24小时
        }
    }
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            data_dir: "./vector_data".to_string(),
            auto_save_interval_seconds: 300, // 5分钟
            compression: true,
            backup: false,
        }
    }
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            default_limit: 10,
            max_limit: 100,
            hybrid_weights: HybridWeights::default(),
            similarity_threshold: 0.5,
        }
    }
}

impl Default for HybridWeights {
    fn default() -> Self {
        Self {
            vector: 0.6,
            keyword: 0.3,
            context: 0.1,
        }
    }
} 