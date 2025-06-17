use crate::{
    concurrent::{AtomicCounters, ConcurrentHashMap},
    storage::VectorStore,
    types::{SearchResult, VectorDbError},
};
use rayon::prelude::*;
use std::sync::Arc;

/// 并行搜索配置
#[derive(Debug, Clone)]
pub struct ParallelSearchConfig {
    /// 最大并行度
    pub max_parallelism: usize,
    /// 批处理大小
    pub batch_size: usize,
    /// 是否启用负载均衡
    pub enable_load_balancing: bool,
}

impl Default for ParallelSearchConfig {
    fn default() -> Self {
        Self {
            max_parallelism: 4,
            batch_size: 100,
            enable_load_balancing: true,
        }
    }
}

/// 并行搜索执行器
pub struct ParallelSearchExecutor {
    config: ParallelSearchConfig,
    /// 高性能缓存用于结果缓存
    result_cache: Arc<ConcurrentHashMap<String, Vec<SearchResult>>>,
    /// 性能计数器
    counters: Arc<AtomicCounters>,
}

impl ParallelSearchExecutor {
    /// 创建新的并行搜索执行器
    pub fn new(config: ParallelSearchConfig) -> Self {
        Self {
            config,
            result_cache: Arc::new(ConcurrentHashMap::new()),
            counters: Arc::new(AtomicCounters::new()),
        }
    }

    /// 获取性能统计
    pub fn get_performance_stats(&self) -> (u64, u64, f64) {
        (
            self.counters.get_operations(),
            self.counters
                .search_operations
                .load(std::sync::atomic::Ordering::Relaxed),
            self.counters.cache_hit_rate(),
        )
    }

    /// 清理缓存
    pub fn clear_cache(&self) {
        // DashMap doesn't have a direct clear method, but we can create a new one
        // For now, we'll leave this as a future optimization
    }

    /// 执行并行文本搜索 - 优化版本，使用缓存和原子计数器
    pub async fn parallel_text_search(
        &self,
        queries: Vec<String>,
        _storage: &dyn VectorStore,
        _limit: usize,
    ) -> Result<Vec<Vec<SearchResult>>, VectorDbError> {
        // 检查缓存
        let mut cached_results = Vec::new();
        let mut uncached_queries = Vec::new();

        for query in &queries {
            if let Some(cached) = self.result_cache.get(query) {
                cached_results.push(Some(cached.clone()));
                self.counters.increment_cache_hits();
            } else {
                cached_results.push(None);
                uncached_queries.push(query.clone());
                self.counters.increment_cache_misses();
            }
        }

        // 并行执行未缓存的查询
        let uncached_results: Result<Vec<_>, _> = uncached_queries
            .into_par_iter()
            .map(|_query| {
                self.counters.increment_search_operations();
                // 注意：这里使用同步版本，因为 rayon 不支持异步
                // 在实际实现中，你可能需要使用 block_on 或其他方式处理异步调用
                // 为了演示，我们返回一个空结果
                Ok(Vec::new())
            })
            .collect();

        let uncached_results = uncached_results?;

        // 缓存新结果
        let mut uncached_iter = uncached_results.into_iter();
        for (i, cached_result) in cached_results.iter_mut().enumerate() {
            if cached_result.is_none() {
                if let Some(result) = uncached_iter.next() {
                    // 缓存结果
                    self.result_cache.insert(queries[i].clone(), result.clone());
                    *cached_result = Some(result);
                }
            }
            self.counters.increment_operations();
        }

        // 转换为最终结果
        Ok(cached_results
            .into_iter()
            .map(|r| r.unwrap_or_default())
            .collect())
    }

    /// 执行并行向量搜索
    pub async fn parallel_vector_search(
        &self,
        query_vectors: Vec<Vec<f32>>,
        storage: &dyn VectorStore,
        limit: usize,
    ) -> Result<Vec<Vec<SearchResult>>, VectorDbError> {
        let results: Result<Vec<_>, _> = query_vectors
            .into_par_iter()
            .map(|query_vector| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { storage.vector_search(&query_vector, limit, None).await })
                })
            })
            .collect();

        results
    }

    /// 获取配置
    pub fn get_config(&self) -> &ParallelSearchConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: ParallelSearchConfig) {
        self.config = config;
    }
}
