use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

use crate::{
    storage::VectorStore,
    types::{SearchResult, VectorDbError},
};

/// 搜索优化器配置
#[derive(Debug, Clone)]
pub struct SearchOptimizerConfig {
    /// 是否启用并行搜索
    pub enable_parallel_search: bool,
    /// 最大并行度
    pub max_parallelism: usize,
    /// 搜索缓存大小
    pub cache_size: usize,
    /// 缓存过期时间（秒）
    pub cache_ttl_seconds: u64,
    /// 是否启用查询优化
    pub enable_query_optimization: bool,
    /// 最小批处理大小
    pub min_batch_size: usize,
}

impl Default for SearchOptimizerConfig {
    fn default() -> Self {
        Self {
            enable_parallel_search: true,
            max_parallelism: 4,
            cache_size: 1000,
            cache_ttl_seconds: 300,
            enable_query_optimization: true,
            min_batch_size: 10,
        }
    }
}

/// 类型别名：搜索结果缓存
type SearchCache = Arc<RwLock<HashMap<String, (Vec<SearchResult>, Instant)>>>;

/// 搜索优化器
pub struct SearchOptimizer {
    config: SearchOptimizerConfig,
    cache: SearchCache,
}

impl SearchOptimizer {
    /// 创建新的搜索优化器
    pub fn new(config: SearchOptimizerConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 优化搜索查询
    pub async fn optimize_search(
        &self,
        query: &str,
        storage: &dyn VectorStore,
    ) -> Result<Vec<SearchResult>, VectorDbError> {
        // 检查缓存
        let cache_key = format!("text:{}", query);
        if let Some(cached_results) = self.get_cached_results(&cache_key) {
            debug!("返回缓存的搜索结果");
            return Ok(cached_results);
        }

        // 执行搜索
        let results = storage.text_search(query, 10, None).await?;

        // 缓存结果
        self.cache_results(&cache_key, results.clone());

        Ok(results)
    }

    /// 获取缓存的结果
    fn get_cached_results(&self, key: &str) -> Option<Vec<SearchResult>> {
        let cache = self.cache.read();
        if let Some((results, timestamp)) = cache.get(key) {
            if timestamp.elapsed().as_secs() < self.config.cache_ttl_seconds {
                return Some(results.clone());
            }
        }
        None
    }

    /// 缓存搜索结果
    fn cache_results(&self, key: &str, results: Vec<SearchResult>) {
        let mut cache = self.cache.write();

        // 清理过期缓存
        let now = Instant::now();
        cache.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp).as_secs() < self.config.cache_ttl_seconds
        });

        // 限制缓存大小
        if cache.len() >= self.config.cache_size {
            // 移除最旧的条目
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(key.to_string(), (results, now));
    }

    /// 清除缓存
    pub fn clear_cache(&self) {
        self.cache.write().clear();
    }

    /// 获取缓存统计信息
    pub fn get_cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read();
        (cache.len(), self.config.cache_size)
    }
}
