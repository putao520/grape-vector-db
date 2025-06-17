use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;

/// 智能缓存管理器
#[allow(dead_code)]
pub struct CacheManager {
    /// 查询结果缓存
    query_cache: Arc<Cache<String, Vec<u8>>>,
    /// 向量嵌入缓存
    embedding_cache: Arc<Cache<String, Vec<f32>>>,
    /// 配置
    config: CacheConfig,
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// 查询缓存大小
    pub query_cache_size: u64,
    /// 嵌入缓存大小
    pub embedding_cache_size: u64,
    /// TTL（秒）
    pub ttl_seconds: u64,
    /// 预热缓存开关
    pub enable_warm_up: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            query_cache_size: 50000,      // 50K 查询
            embedding_cache_size: 100000, // 100K 嵌入
            ttl_seconds: 1800,            // 30分钟
            enable_warm_up: true,
        }
    }
}

impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        let query_cache = Arc::new(
            Cache::builder()
                .max_capacity(config.query_cache_size)
                .time_to_live(Duration::from_secs(config.ttl_seconds))
                .build(),
        );

        let embedding_cache = Arc::new(
            Cache::builder()
                .max_capacity(config.embedding_cache_size)
                .time_to_live(Duration::from_secs(config.ttl_seconds))
                .build(),
        );

        Self {
            query_cache,
            embedding_cache,
            config,
        }
    }

    /// 获取查询缓存
    pub fn get_query_result(&self, key: &str) -> Option<Vec<u8>> {
        self.query_cache.get(key)
    }

    /// 设置查询缓存
    pub fn set_query_result(&self, key: String, value: Vec<u8>) {
        self.query_cache.insert(key, value);
    }

    /// 获取嵌入缓存
    pub fn get_embedding(&self, key: &str) -> Option<Vec<f32>> {
        self.embedding_cache.get(key)
    }

    /// 设置嵌入缓存
    pub fn set_embedding(&self, key: String, value: Vec<f32>) {
        self.embedding_cache.insert(key, value);
    }

    /// 获取缓存统计
    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            query_cache_size: self.query_cache.entry_count(),
            embedding_cache_size: self.embedding_cache.entry_count(),
            query_hit_ratio: 0.0,
            embedding_hit_ratio: 0.0,
        }
    }
}

/// 缓存统计信息
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub query_cache_size: u64,
    pub embedding_cache_size: u64,
    pub query_hit_ratio: f64,
    pub embedding_hit_ratio: f64,
}
