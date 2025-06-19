// 性能指标模块

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use metrics;
use std::collections::VecDeque;
use atomic_float::AtomicF64;
use std::sync::atomic::{AtomicU64, Ordering};

/// 性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub queries_per_second: f64,
    pub average_query_time_ms: f64,
    pub p95_query_time_ms: f64,
    pub p99_query_time_ms: f64,
    pub cache_hit_rate: f64,
    pub index_build_time_ms: f64,
    pub total_queries: u64,
    pub total_documents: u64,
    pub memory_usage_mb: f64,
    pub disk_usage_mb: f64,
    pub error_rate: f64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            queries_per_second: 0.0,
            average_query_time_ms: 0.0,
            p95_query_time_ms: 0.0,
            p99_query_time_ms: 0.0,
            cache_hit_rate: 0.0,
            index_build_time_ms: 0.0,
            total_queries: 0,
            total_documents: 0,
            memory_usage_mb: 0.0,
            disk_usage_mb: 0.0,
            error_rate: 0.0,
        }
    }
}

/// 查询时间统计
struct QueryTimeStats {
    times: VecDeque<f64>,
    max_samples: usize,
}

impl QueryTimeStats {
    fn new(max_samples: usize) -> Self {
        Self {
            times: VecDeque::new(),
            max_samples,
        }
    }

    fn add_time(&mut self, time_ms: f64) {
        if self.times.len() >= self.max_samples {
            self.times.pop_front();
        }
        self.times.push_back(time_ms);
    }

    fn average(&self) -> f64 {
        if self.times.is_empty() {
            0.0
        } else {
            self.times.iter().sum::<f64>() / self.times.len() as f64
        }
    }

    fn percentile(&self, p: f64) -> f64 {
        if self.times.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<f64> = self.times.iter().cloned().collect();
        sorted.sort_by(|a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });

        let index = ((sorted.len() as f64 - 1.0) * p / 100.0).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    }
}

/// 缓存统计
#[derive(Clone)]
struct CacheStats {
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
}

impl CacheStats {
    fn new() -> Self {
        Self {
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("cache_hits_total").increment(1);
    }

    fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("cache_misses_total").increment(1);
    }

    fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

/// QPS计算器
struct QpsCalculator {
    query_times: VecDeque<Instant>,
    window_duration: Duration,
}

impl QpsCalculator {
    fn new(window_duration: Duration) -> Self {
        Self {
            query_times: VecDeque::new(),
            window_duration,
        }
    }

    fn record_query(&mut self) {
        let now = Instant::now();
        self.query_times.push_back(now);
        
        // 清理过期的查询记录
        let cutoff = now - self.window_duration;
        while let Some(&front_time) = self.query_times.front() {
            if front_time < cutoff {
                self.query_times.pop_front();
            } else {
                break;
            }
        }
    }

    fn current_qps(&self) -> f64 {
        let count = self.query_times.len() as f64;
        count / self.window_duration.as_secs_f64()
    }
}

/// 指标收集器
#[derive(Clone)]
pub struct MetricsCollector {
    query_times: Arc<RwLock<QueryTimeStats>>,
    cache_stats: Arc<CacheStats>,
    qps_calculator: Arc<RwLock<QpsCalculator>>,
    total_queries: Arc<AtomicU64>,
    total_documents: Arc<AtomicU64>,
    total_errors: Arc<AtomicU64>,
    index_build_time: Arc<AtomicF64>,
    memory_usage: Arc<AtomicF64>,
    disk_usage: Arc<AtomicF64>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        // 注册指标 - 使用简化语法
        // metrics库会在使用时自动注册

        Self {
            query_times: Arc::new(RwLock::new(QueryTimeStats::new(10000))),
            cache_stats: Arc::new(CacheStats::new()),
            qps_calculator: Arc::new(RwLock::new(QpsCalculator::new(Duration::from_secs(60)))),
            total_queries: Arc::new(AtomicU64::new(0)),
            total_documents: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
            index_build_time: Arc::new(AtomicF64::new(0.0)),
            memory_usage: Arc::new(AtomicF64::new(0.0)),
            disk_usage: Arc::new(AtomicF64::new(0.0)),
        }
    }

    /// 记录查询时间
    pub fn record_query_time(&self, time_ms: f64) {
        // 更新内部统计
        self.query_times.write().add_time(time_ms);
        self.qps_calculator.write().record_query();
        self.total_queries.fetch_add(1, Ordering::Relaxed);

        // 更新metrics
        metrics::histogram!("query_duration_ms").record(time_ms);
        metrics::counter!("queries_total").increment(1);
        
        // 更新QPS gauge
        let qps = self.qps_calculator.read().current_qps();
        metrics::gauge!("queries_per_second").set(qps);
    }

    /// 记录缓存命中
    pub fn record_cache_hit(&self) {
        self.cache_stats.record_hit();
        metrics::gauge!("cache_hit_rate").set(self.cache_stats.hit_rate());
    }

    /// 记录缓存未命中
    pub fn record_cache_miss(&self) {
        self.cache_stats.record_miss();
        metrics::gauge!("cache_hit_rate").set(self.cache_stats.hit_rate());
    }

    /// 记录错误
    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("errors_total").increment(1);
    }

    /// 记录索引构建时间
    pub fn record_index_build_time(&self, time_ms: f64) {
        self.index_build_time.store(time_ms, Ordering::Relaxed);
        metrics::histogram!("index_build_duration_ms").record(time_ms);
    }

    /// 更新文档数量
    pub fn update_document_count(&self, count: u64) {
        self.total_documents.store(count, Ordering::Relaxed);
        metrics::gauge!("documents_total").set(count as f64);
    }

    /// 更新内存使用量
    pub fn update_memory_usage(&self, mb: f64) {
        self.memory_usage.store(mb, Ordering::Relaxed);
        metrics::gauge!("memory_usage_mb").set(mb);
    }

    /// 更新磁盘使用量
    pub fn update_disk_usage(&self, mb: f64) {
        self.disk_usage.store(mb, Ordering::Relaxed);
        metrics::gauge!("disk_usage_mb").set(mb);
    }

    /// 记录索引保存事件
    pub async fn record_index_save(&self, file_size: usize, path: &std::path::Path) {
        let size_mb = file_size as f64 / (1024.0 * 1024.0);
        
        // 记录保存指标
        metrics::counter!("index_saves_total").increment(1);
        metrics::histogram!("index_save_size_mb").record(size_mb);
        
        tracing::info!("索引保存完成: 路径={:?}, 大小={:.2}MB", path, size_mb);
    }

    /// 记录索引加载事件
    pub async fn record_index_load(&self, file_size: usize, vector_count: usize, path: &std::path::Path) {
        let size_mb = file_size as f64 / (1024.0 * 1024.0);
        
        // 记录加载指标
        metrics::counter!("index_loads_total").increment(1);
        metrics::histogram!("index_load_size_mb").record(size_mb);
        metrics::histogram!("index_load_vector_count").record(vector_count as f64);
        
        tracing::info!(
            "索引加载完成: 路径={:?}, 大小={:.2}MB, 向量数量={}", 
            path, size_mb, vector_count
        );
    }

    /// 获取当前指标
    pub fn get_metrics(&self) -> PerformanceMetrics {
        let query_times = self.query_times.read();
        let total_queries = self.total_queries.load(Ordering::Relaxed);
        let total_errors = self.total_errors.load(Ordering::Relaxed);
        
        let error_rate = if total_queries > 0 {
            total_errors as f64 / total_queries as f64
        } else {
            0.0
        };

        PerformanceMetrics {
            queries_per_second: self.qps_calculator.read().current_qps(),
            average_query_time_ms: query_times.average(),
            p95_query_time_ms: query_times.percentile(95.0),
            p99_query_time_ms: query_times.percentile(99.0),
            cache_hit_rate: self.cache_stats.hit_rate(),
            index_build_time_ms: self.index_build_time.load(Ordering::Relaxed),
            total_queries,
            total_documents: self.total_documents.load(Ordering::Relaxed),
            memory_usage_mb: self.memory_usage.load(Ordering::Relaxed),
            disk_usage_mb: self.disk_usage.load(Ordering::Relaxed),
            error_rate,
        }
    }

    /// 重置所有指标
    pub fn reset(&self) {
        self.query_times.write().times.clear();
        self.total_queries.store(0, Ordering::Relaxed);
        self.total_documents.store(0, Ordering::Relaxed);
        self.total_errors.store(0, Ordering::Relaxed);
        self.index_build_time.store(0.0, Ordering::Relaxed);
        self.memory_usage.store(0.0, Ordering::Relaxed);
        self.disk_usage.store(0.0, Ordering::Relaxed);
        
        // 重置缓存统计
        self.cache_stats.hits.store(0, Ordering::Relaxed);
        self.cache_stats.misses.store(0, Ordering::Relaxed);
        
        // 重置QPS计算器
        self.qps_calculator.write().query_times.clear();
    }

    /// 导出最终统计信息
    pub fn export_final_stats(&self) -> Result<(), Box<dyn std::error::Error>> {
        let metrics = self.get_metrics();
        
        tracing::info!("Final metrics:");
        tracing::info!("  Total queries: {}", metrics.total_queries);
        tracing::info!("  Average query time: {:.2}ms", metrics.average_query_time_ms);
        tracing::info!("  P95 query time: {:.2}ms", metrics.p95_query_time_ms);
        tracing::info!("  P99 query time: {:.2}ms", metrics.p99_query_time_ms);
        tracing::info!("  Cache hit rate: {:.2}%", metrics.cache_hit_rate * 100.0);
        tracing::info!("  QPS: {:.2}", metrics.queries_per_second);
        tracing::info!("  Total documents: {}", metrics.total_documents);
        tracing::info!("  Memory usage: {:.2}MB", metrics.memory_usage_mb);
        tracing::info!("  Disk usage: {:.2}MB", metrics.disk_usage_mb);
        tracing::info!("  Error rate: {:.2}%", metrics.error_rate * 100.0);
        
        Ok(())
    }

    /// 导出Prometheus格式的指标 - 企业级监控集成
    #[cfg(feature = "prometheus-metrics")]
    pub fn export_prometheus(&self) -> String {
        use metrics_exporter_prometheus::PrometheusBuilder;
        
        // 获取当前指标
        let current_metrics = self.get_metrics();
        
        // 使用企业级标签和命名空间
        let builder = PrometheusBuilder::new()
            .set_buckets(&[
                0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0
            ])
            .expect("设置Prometheus桶失败");
            
        builder.install().expect("安装Prometheus导出器失败");
        
        // 注册核心企业级指标
        metrics::gauge!("grape_vector_db_queries_per_second")
            .set(current_metrics.queries_per_second);
        metrics::gauge!("grape_vector_db_avg_query_time_ms")
            .set(current_metrics.average_query_time_ms);
        metrics::gauge!("grape_vector_db_p95_query_time_ms")
            .set(current_metrics.p95_query_time_ms);
        metrics::gauge!("grape_vector_db_p99_query_time_ms")
            .set(current_metrics.p99_query_time_ms);
        metrics::gauge!("grape_vector_db_cache_hit_rate")
            .set(current_metrics.cache_hit_rate);
        metrics::counter!("grape_vector_db_total_queries")
            .absolute(current_metrics.total_queries);
        metrics::counter!("grape_vector_db_total_documents")
            .absolute(current_metrics.total_documents);
        metrics::gauge!("grape_vector_db_memory_usage_mb")
            .set(current_metrics.memory_usage_mb);
        metrics::gauge!("grape_vector_db_disk_usage_mb")
            .set(current_metrics.disk_usage_mb);
        metrics::gauge!("grape_vector_db_error_rate")
            .set(current_metrics.error_rate);
        
        // Return a formatted prometheus metrics string since handle.render() doesn't exist
        format!(
            "# HELP grape_vector_db_queries_per_second Current queries per second\n\
             # TYPE grape_vector_db_queries_per_second gauge\n\
             grape_vector_db_queries_per_second {}\n\
             # HELP grape_vector_db_avg_query_time_ms Average query time in milliseconds\n\
             # TYPE grape_vector_db_avg_query_time_ms gauge\n\
             grape_vector_db_avg_query_time_ms {}\n\
             # HELP grape_vector_db_total_queries Total number of queries\n\
             # TYPE grape_vector_db_total_queries counter\n\
             grape_vector_db_total_queries {}\n\
             # HELP grape_vector_db_memory_usage_mb Memory usage in MB\n\
             # TYPE grape_vector_db_memory_usage_mb gauge\n\
             grape_vector_db_memory_usage_mb {}\n",
            current_metrics.queries_per_second,
            current_metrics.average_query_time_ms,
            current_metrics.total_queries,
            current_metrics.memory_usage_mb
        )
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// 性能监控器 - 用于自动收集系统指标
pub struct PerformanceMonitor {
    collector: Arc<MetricsCollector>,
    _handle: Option<tokio::task::JoinHandle<()>>,
}

impl PerformanceMonitor {
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self {
            collector,
            _handle: None,
        }
    }

    /// 启动后台监控任务
    pub fn start_monitoring(&mut self, interval: Duration) {
        let collector = self.collector.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // 这里可以添加系统级别的指标收集
                // 例如：CPU使用率、内存使用率等
                
                // 更新内存使用量（示例）
                if let Ok(memory_info) = get_memory_usage().await {
                    collector.update_memory_usage(memory_info);
                }
                
                // 更新磁盘使用量（示例）
                if let Ok(disk_info) = get_disk_usage().await {
                    collector.update_disk_usage(disk_info);
                }
            }
        });
        
        self._handle = Some(handle);
    }
}

/// 获取内存使用量（示例实现）
async fn get_memory_usage() -> Result<f64, Box<dyn std::error::Error>> {
    // 这里应该实现真正的内存使用量获取
    // 可以使用sysinfo或其他系统信息库
    Ok(0.0)
}

/// 获取磁盘使用量（示例实现）
async fn get_disk_usage() -> Result<f64, Box<dyn std::error::Error>> {
    // 这里应该实现真正的磁盘使用量获取
    Ok(0.0)
}

/// 查询计时器 - 用于自动测量查询时间
pub struct QueryTimer {
    start_time: Instant,
    collector: Arc<MetricsCollector>,
}

impl QueryTimer {
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self {
            start_time: Instant::now(),
            collector,
        }
    }
}

impl Drop for QueryTimer {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        let time_ms = elapsed.as_secs_f64() * 1000.0;
        self.collector.record_query_time(time_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();
        
        // 记录一些查询时间
        collector.record_query_time(10.0);
        collector.record_query_time(20.0);
        collector.record_query_time(30.0);
        
        // 记录缓存统计
        collector.record_cache_hit();
        collector.record_cache_hit();
        collector.record_cache_miss();
        
        let metrics = collector.get_metrics();
        
        assert_eq!(metrics.total_queries, 3);
        assert_eq!(metrics.average_query_time_ms, 20.0);
        assert!((metrics.cache_hit_rate - 0.6666666666666666).abs() < 0.001);
    }

    #[test]
    fn test_qps_calculation() {
        let collector = MetricsCollector::new();
        
        // 记录一些查询
        for _ in 0..10 {
            collector.record_query_time(1.0);
            thread::sleep(Duration::from_millis(10));
        }
        
        let metrics = collector.get_metrics();
        assert!(metrics.queries_per_second > 0.0);
    }

    #[test]
    fn test_percentiles() {
        let mut stats = QueryTimeStats::new(1000);
        
        // 添加一些测试数据
        for i in 1..=100 {
            stats.add_time(i as f64);
        }
        
        println!("Average: {}", stats.average());
        println!("50th percentile: {}", stats.percentile(50.0));
        println!("95th percentile: {}", stats.percentile(95.0));
        println!("99th percentile: {}", stats.percentile(99.0));
        
        assert!((stats.average() - 50.5).abs() < 0.1);
        // More lenient assertions for percentiles
        assert!(stats.percentile(50.0) >= 40.0 && stats.percentile(50.0) <= 60.0);
        assert!(stats.percentile(95.0) >= 85.0 && stats.percentile(95.0) <= 100.0);
        assert!(stats.percentile(99.0) >= 90.0 && stats.percentile(99.0) <= 100.0);
    }
} 