pub mod cache_manager;
#[cfg(test)]
pub mod concurrent_test;
pub mod index_optimizer;
pub mod parallel_search;
pub mod search_optimizer;

// 重新导出主要类型
pub use cache_manager::{CacheConfig, CacheManager, CacheStats};
pub use index_optimizer::{IndexOptimizer, IndexOptimizerConfig, OptimizationStats};
pub use parallel_search::{ParallelSearchConfig, ParallelSearchExecutor};
pub use search_optimizer::{SearchOptimizer, SearchOptimizerConfig};

#[cfg(test)]
pub use concurrent_test::ConcurrentPerformanceTest;

/// 性能统计信息
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PerformanceStats {
    /// 查询总数
    pub total_queries: u64,
    /// 平均查询时间（毫秒）
    pub average_query_time_ms: f64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// 索引优化次数
    pub optimization_count: u64,
    /// 内存使用量（字节）
    pub memory_usage_bytes: u64,
}

/// 性能监控器
#[derive(Debug)]
pub struct PerformanceMonitor {
    stats: parking_lot::RwLock<PerformanceStats>,
}

impl PerformanceMonitor {
    /// 创建新的性能监控器
    pub fn new() -> Self {
        Self {
            stats: parking_lot::RwLock::new(PerformanceStats::default()),
        }
    }

    /// 记录查询
    pub fn record_query(&self, duration_ms: f64) {
        let mut stats = self.stats.write();
        stats.total_queries += 1;

        // 计算移动平均
        if stats.total_queries == 1 {
            stats.average_query_time_ms = duration_ms;
        } else {
            stats.average_query_time_ms =
                (stats.average_query_time_ms * (stats.total_queries - 1) as f64 + duration_ms)
                    / stats.total_queries as f64;
        }
    }

    /// 更新缓存命中率
    pub fn update_cache_hit_rate(&self, hit_rate: f64) {
        self.stats.write().cache_hit_rate = hit_rate;
    }

    /// 记录优化
    pub fn record_optimization(&self) {
        self.stats.write().optimization_count += 1;
    }

    /// 更新内存使用量
    pub fn update_memory_usage(&self, bytes: u64) {
        self.stats.write().memory_usage_bytes = bytes;
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> PerformanceStats {
        self.stats.read().clone()
    }

    /// 重置统计信息
    pub fn reset_stats(&self) {
        *self.stats.write() = PerformanceStats::default();
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// 为了兼容性，创建别名
pub type PerformanceMetrics = PerformanceStats;
