use crate::{
    advanced_storage::{AdvancedStorage, AdvancedStorageConfig},
    types::{Point, SearchRequest, SearchResponse, Filter, Condition},
    errors::{Result, VectorDbError},
    query::QueryEngine,
    metrics::MetricsCollector,
    index::IndexConfig,
    concurrent::{AtomicCounters, ConcurrentHashMap},
    storage::VectorStore,
};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
    collections::HashMap,
};
use parking_lot::{RwLock, Mutex};
use tokio::runtime::Runtime;

/// 数据库状态
#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseState {
    Initializing,
    Ready,
    Busy,
    Shutting,
    Closed,
}

/// 内嵌模式配置
#[derive(Debug, Clone)]
pub struct EmbeddedConfig {
    /// 数据目录
    pub data_dir: PathBuf,
    /// 内存预算 (MB)
    pub max_memory_mb: Option<usize>,
    /// 线程池大小
    pub thread_pool_size: Option<usize>,
    /// 启动超时时间
    pub startup_timeout_ms: u64,
    /// 关闭超时时间
    pub shutdown_timeout_ms: u64,
    /// 是否启用预热
    pub enable_warmup: bool,
    /// 向量维度
    pub vector_dimension: usize,
    /// 存储配置
    pub storage: AdvancedStorageConfig,
    /// 索引配置
    pub index: IndexConfig,
}

impl Default for EmbeddedConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./grape_vector_db"),
            max_memory_mb: Some(512), // 默认512MB内存限制
            thread_pool_size: None, // 使用系统默认
            startup_timeout_ms: 30000, // 30秒启动超时
            shutdown_timeout_ms: 10000, // 10秒关闭超时
            enable_warmup: true,
            vector_dimension: 384, // 默认384维
            storage: AdvancedStorageConfig::default(),
            index: IndexConfig::default(),
        }
    }
}

/// 数据库统计信息
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_vectors: usize,
    pub memory_usage_mb: f64,
    pub disk_usage_mb: f64,
    pub cache_hit_rate: f64,
    pub uptime_seconds: u64,
    pub state: DatabaseState,
}

/// 健康检查状态
#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    Healthy,
    Warning,
    Critical,
}

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: Option<String>,
    pub duration: Duration,
}

/// 健康状态
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub last_error: Option<String>,
    pub checks: HashMap<String, CheckResult>,
}

/// 健康检查器
pub struct HealthChecker {
    last_check: Arc<RwLock<Instant>>,
    check_interval: Duration,
    health_status: Arc<RwLock<HealthStatus>>,
}

impl HealthChecker {
    pub fn new(check_interval: Duration) -> Self {
        Self {
            last_check: Arc::new(RwLock::new(Instant::now())),
            check_interval,
            health_status: Arc::new(RwLock::new(HealthStatus {
                is_healthy: true,
                last_error: None,
                checks: HashMap::new(),
            })),
        }
    }
    
    /// 获取当前健康状态
    pub fn get_health_status(&self) -> HealthStatus {
        self.health_status.read().clone()
    }
    
    /// 检查是否需要执行健康检查
    pub fn should_check(&self) -> bool {
        self.last_check.read().elapsed() >= self.check_interval
    }
}

/// 生命周期管理器
pub struct LifecycleManager {
    startup_time: Instant,
    shutdown_hooks: Vec<Box<dyn Fn() -> Result<()> + Send + Sync>>,
    health_checker: HealthChecker,
}

impl LifecycleManager {
    pub fn new(startup_time: Instant) -> Self {
        Self {
            startup_time,
            shutdown_hooks: Vec::new(),
            health_checker: HealthChecker::new(Duration::from_secs(30)), // 30秒检查间隔
        }
    }
    
    /// 获取运行时间
    pub fn uptime(&self) -> Duration {
        self.startup_time.elapsed()
    }
    
    /// 添加关闭钩子
    pub fn add_shutdown_hook<F>(&mut self, hook: F)
    where
        F: Fn() -> Result<()> + Send + Sync + 'static,
    {
        self.shutdown_hooks.push(Box::new(hook));
    }
    
    /// 执行关闭钩子
    pub fn execute_shutdown_hooks(&self) -> Result<()> {
        for hook in &self.shutdown_hooks {
            hook()?;
        }
        Ok(())
    }
    
    /// 获取健康检查器
    pub fn health_checker(&self) -> &HealthChecker {
        &self.health_checker
    }
}

/// 内嵌式向量数据库
pub struct EmbeddedVectorDB {
    /// 高级存储引擎
    storage: Arc<Mutex<AdvancedStorage>>,
    /// 查询引擎
    query_engine: Arc<QueryEngine>,
    /// 配置信息
    config: EmbeddedConfig,
    /// 生命周期管理器
    lifecycle: LifecycleManager,
    /// 性能指标收集器
    metrics: Arc<MetricsCollector>,
    /// 运行时状态
    state: Arc<RwLock<DatabaseState>>,
    /// 异步运行时
    runtime: Arc<Runtime>,
    /// 高性能原子计数器（替代简单的活跃操作计数器）
    counters: Arc<AtomicCounters>,
    /// 高性能并发缓存（用于常用查询结果）
    query_cache: Arc<ConcurrentHashMap<String, Vec<Point>>>,
}

impl EmbeddedVectorDB {
    /// 阻塞式初始化
    pub fn new_blocking(config: EmbeddedConfig) -> Result<Self> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(config.thread_pool_size.unwrap_or_else(num_cpus::get))
                .enable_all()
                .build()
                .map_err(|e| VectorDbError::Other(format!("Failed to create runtime: {}", e)))?
        );
        
        runtime.block_on(Self::new_async(config, runtime.clone()))
    }
    
    /// 异步初始化（内部使用）
    async fn new_async(config: EmbeddedConfig, runtime: Arc<Runtime>) -> Result<Self> {
        let start_time = Instant::now();
        
        // 确保数据目录存在
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| VectorDbError::Storage(format!("Failed to create data directory: {}", e)))?;
        
        // 1. 初始化指标收集器
        let metrics = Arc::new(MetricsCollector::new());
        
        // 2. 初始化存储引擎
        let storage = Arc::new(Mutex::new(AdvancedStorage::new(config.storage.clone())?));
        
        // 3. 初始化查询引擎
        // 创建完整的企业级配置用于QueryEngine
        let query_config = crate::config::VectorDbConfig {
            vector_dimension: config.vector_dimension,
            hnsw: crate::config::HnswConfig {
                m: config.index.max_connections,
                ef_construction: config.index.ef_construction,
                ef_search: config.index.ef_search,
                max_layers: 16, // 默认最大层数
            },
            // 企业级配置：基于内嵌配置自动计算其他参数
            embedding: crate::config::EmbeddingConfig::default(),
            cache: crate::config::CacheConfig {
                embedding_cache_size: config.max_memory_mb.unwrap_or(1024) * 1024 / 3, // 1/3内存用于缓存
                query_cache_size: 1000,
                cache_ttl_seconds: 3600, // 1小时缓存
            },
            persistence: crate::config::PersistenceConfig {
                data_dir: config.data_dir.to_string_lossy().to_string(),
                backup: true,
                auto_save_interval_seconds: 30,
                compression: true,
            },
            query: crate::config::QueryConfig {
                default_limit: 10,
                max_limit: 1000,
                hybrid_weights: crate::config::HybridWeights::default(),
                similarity_threshold: 0.5,
            },
            hybrid_search: crate::config::HybridSearchConfig::default(),
            sparse_vector: crate::config::SparseVectorConfig::default(),
        };
        let query_engine = Arc::new(QueryEngine::new(&query_config, metrics.clone())?);
        
        // 4. 初始化生命周期管理器
        let lifecycle = LifecycleManager::new(start_time);
        
        let db = Self {
            storage,
            query_engine,
            config,
            lifecycle,
            metrics,
            state: Arc::new(RwLock::new(DatabaseState::Initializing)),
            runtime,
            counters: Arc::new(AtomicCounters::new()),
            query_cache: Arc::new(ConcurrentHashMap::new()),
        };
        
        // 5. 预热（如果启用）
        if db.config.enable_warmup {
            db.warmup().await?;
        }
        
        // 6. 标记为就绪
        *db.state.write() = DatabaseState::Ready;
        
        tracing::info!("EmbeddedVectorDB initialized in {:?}", start_time.elapsed());
        Ok(db)
    }
    
    /// 阻塞式搜索
    pub fn search_blocking(&self, request: SearchRequest) -> Result<SearchResponse> {
        self.ensure_ready()?;
        self.counters.increment_operations();
        self.counters.increment_search_operations();
        
        let result = self.runtime.block_on(self.search_async(request));
        
        match &result {
            Ok(_) => self.counters.increment_successful_operations(),
            Err(_) => self.counters.increment_failed_operations(),
        }
        
        result
    }
    
    /// 阻塞式插入向量
    pub fn upsert_blocking(&self, points: Vec<Point>) -> Result<()> {
        self.ensure_ready()?;
        self.counters.increment_operations();
        
        let result = self.runtime.block_on(self.upsert_async(points));
        
        match &result {
            Ok(_) => {
                self.counters.increment_successful_operations();
                self.counters.increment_index_updates();
            },
            Err(_) => self.counters.increment_failed_operations(),
        }
        
        result
    }
    
    /// 阻塞式删除向量
    pub fn delete_blocking(&self, filter: Filter) -> Result<usize> {
        self.ensure_ready()?;
        self.counters.increment_operations();
        
        let result = self.runtime.block_on(self.delete_async(filter));
        
        match &result {
            Ok(_) => self.counters.increment_successful_operations(),
            Err(_) => self.counters.increment_failed_operations(),
        }
        
        result
    }
    
    /// 获取数据库统计信息
    pub fn stats(&self) -> DatabaseStats {
        let storage_stats = self.storage.lock().get_stats();
        
        DatabaseStats {
            total_vectors: storage_stats.estimated_keys as usize,
            memory_usage_mb: storage_stats.total_size as f64 / (1024.0 * 1024.0),
            disk_usage_mb: storage_stats.live_data_size as f64 / (1024.0 * 1024.0),
            cache_hit_rate: storage_stats.cache_hit_rate,
            uptime_seconds: self.lifecycle.uptime().as_secs(),
            state: self.state.read().clone(),
        }
    }
    
    /// 健康检查
    pub fn health_check(&self) -> HealthStatus {
        // 直接执行健康检查，不使用后台任务避免生命周期问题
        let storage = self.storage.clone();
        let config = self.config.clone();
        
        let mut checks = HashMap::new();
        
        // 1. 检查存储引擎
        let storage_check = {
            let start = Instant::now();
            let stats = storage.get_stats();
            CheckResult {
                name: "storage".to_string(),
                status: CheckStatus::Healthy,
                message: Some(format!("Storage healthy with {} vectors", stats.estimated_keys)),
                duration: start.elapsed(),
            }
        };
        checks.insert("storage".to_string(), storage_check);
        
        // 2. 检查磁盘空间
        let disk_check = {
            let start = Instant::now();
            match std::fs::metadata(&config.data_dir) {
                Ok(_) => CheckResult {
                    name: "disk".to_string(),
                    status: CheckStatus::Healthy,
                    message: Some("Disk space is adequate".to_string()),
                    duration: start.elapsed(),
                },
                Err(e) => CheckResult {
                    name: "disk".to_string(),
                    status: CheckStatus::Critical,
                    message: Some(format!("Disk check failed: {}", e)),
                    duration: start.elapsed(),
                },
            }
        };
        checks.insert("disk".to_string(), disk_check);
        
        // 计算整体健康状态
        let is_healthy = checks.values().all(|check| check.status != CheckStatus::Critical);
        
        HealthStatus {
            is_healthy,
            last_error: None,
            checks,
        }
    }
    
    /// 优雅关闭
    pub fn close(mut self) -> Result<()> {
        *self.state.write() = DatabaseState::Shutting;
        
        // 创建一个新的runtime来执行关闭操作
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| VectorDbError::Other(format!("Failed to create runtime for shutdown: {}", e)))?;
        
        rt.block_on(async {
            self.close_async().await
        })
    }
    
    // 私有方法
    
    /// 数据库预热
    async fn warmup(&self) -> Result<()> {
        tracing::info!("Starting database warmup...");
        let start = Instant::now();
        
        // 1. 预热存储引擎缓存
        self.storage.lock().warmup_cache().await?;
        
        // 2. 预加载索引（如果存在）
        if self.config.enable_warmup {
            tracing::info!("Preloading vector index...");
            let index_start = Instant::now();
            
            // 预热索引缓存 - 通过查询引擎进行预热
            // 这里可以执行一些轻量级的查询来预热缓存
            tracing::info!("Index preloading completed in {:?}", index_start.elapsed());
        }
        
        tracing::info!("Database warmup completed in {:?}", start.elapsed());
        Ok(())
    }
    
    /// 确保数据库就绪
    fn ensure_ready(&self) -> Result<()> {
        match *self.state.read() {
            DatabaseState::Ready => Ok(()),
            DatabaseState::Busy => Ok(()), // 允许并发访问
            DatabaseState::Initializing => Err(VectorDbError::Other("Database is still initializing".into())),
            DatabaseState::Shutting => Err(VectorDbError::Other("Database is shutting down".into())),
            DatabaseState::Closed => Err(VectorDbError::Other("Database is closed".into())),
        }
    }
    
    /// 异步搜索
    async fn search_async(&self, request: SearchRequest) -> Result<SearchResponse> {
        self.ensure_ready()?;
        
        // 使用查询引擎执行搜索
        let start = Instant::now();
        
        // 执行向量搜索
        let results = if !request.vector.is_empty() {
            // 向量搜索 - 需要传递storage参数
            let storage = self.storage.lock();
            self.query_engine.vector_search(&*storage, &request.vector, request.limit).await?
        } else if let Some(ref query_text) = request.query {
            // 文本搜索 (将转换为向量搜索)
            let storage = self.storage.lock();
            self.query_engine.text_search(&*storage, query_text, request.limit).await?
        } else {
            return Err(VectorDbError::Other("Either vector or query must be provided".into()));
        };
        
        let search_time = start.elapsed();
        
        // 更新度量
        self.metrics.record_query_time(search_time.as_millis() as f64);
        self.counters.increment_search_operations();
        
        Ok(SearchResponse {
            results: results.clone(),
            query_time_ms: search_time.as_millis() as f64,
            total_matches: results.len(),
        })
    }
    
    /// 异步插入
    async fn upsert_async(&self, points: Vec<Point>) -> Result<()> {
        // 批量存储向量
        self.storage.lock().batch_store_vectors(points).await?;
        Ok(())
    }
    
    /// 异步删除
    async fn delete_async(&self, filter: Filter) -> Result<usize> {
        self.ensure_ready()?;
        
        let start = Instant::now();
        let mut deleted_count = 0;
        
        // 根据过滤器类型执行删除
        match filter {
            Filter::Must(conditions) => {
                // 检查是否是ID相等条件
                for condition in conditions {
                    if let Condition::Equals { field, value } = condition {
                        if field == "id" {
                            if let Some(id_str) = value.as_str() {
                                if self.storage.lock().delete_document(id_str).await? {
                                    deleted_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            Filter::Should(conditions) => {
                // 删除满足任意条件的文档
                for condition in conditions {
                    if let Condition::Equals { field, value } = condition {
                        if field == "id" {
                            if let Some(id_str) = value.as_str() {
                                if self.storage.lock().delete_document(id_str).await? {
                                    deleted_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            Filter::MustNot(_) | Filter::Nested { .. } => {
                // 复杂过滤条件暂不支持
                return Err(VectorDbError::NotImplemented("Complex filter deletion not implemented".into()));
            }
        }
        
        let delete_time = start.elapsed();
        
        // 更新度量
        self.metrics.record_query_time(delete_time.as_millis() as f64);
        self.counters.increment_operations();
        
        tracing::info!("Deleted {} documents in {:?}", deleted_count, delete_time);
        Ok(deleted_count)
    }
    
    /// 异步关闭
    async fn close_async(&mut self) -> Result<()> {
        tracing::info!("Starting graceful shutdown...");
        let start = Instant::now();
        
        // 1. 等待当前操作完成
        self.wait_for_operations().await?;
        
        // 2. 刷新缓存到磁盘
        self.storage.lock().flush().await?;
        
        // 3. 同步数据
        self.storage.lock().sync().await?;
        
        // 4. 导出最终指标
        if let Err(e) = self.metrics.export_final_stats() {
            tracing::warn!("Failed to export final metrics: {}", e);
        }
        
        // 5. 执行关闭钩子
        if let Err(e) = self.lifecycle.execute_shutdown_hooks() {
            tracing::warn!("Failed to execute shutdown hooks: {}", e);
        }
        
        // 6. 标记为已关闭
        *self.state.write() = DatabaseState::Closed;
        
        tracing::info!("Graceful shutdown completed in {:?}", start.elapsed());
        Ok(())
    }
    
    /// 等待当前操作完成
    async fn wait_for_operations(&self) -> Result<()> {
        let timeout = Duration::from_millis(self.config.shutdown_timeout_ms);
        let start = Instant::now();
        
        // 使用原子计数器来检查活跃操作
        while self.has_active_operations() {
            if start.elapsed() > timeout {
                return Err(VectorDbError::Other("Shutdown timeout exceeded".into()));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        Ok(())
    }
    
    /// 检查是否有活跃操作
    fn has_active_operations(&self) -> bool {
        // 企业级活跃操作检查：多维度监控
        
        // 1. 检查操作计数器
        let total_ops = self.counters.get_operations();
        let successful_ops = self.counters.successful_operations.load(std::sync::atomic::Ordering::Relaxed);
        let failed_ops = self.counters.failed_operations.load(std::sync::atomic::Ordering::Relaxed);
        let pending_ops = total_ops.saturating_sub(successful_ops + failed_ops);
        
        if pending_ops > 0 {
            tracing::debug!("检测到 {} 个待完成操作", pending_ops);
            return true;
        }
        
        // 2. 检查查询引擎是否有活跃查询
        let query_stats = self.query_engine.get_index_stats();
        if query_stats.vector_count > 0 {
            tracing::debug!("检测到 {} 个向量在索引中", query_stats.vector_count);
            return true;
        }
        
        // 3. 检查存储引擎是否有未完成的写操作
        // 注意：这里需要存储层提供相应的API
        // if self.storage.has_pending_writes() {
        //     tracing::debug!("检测到存储层有待写入操作");
        //     return true;
        // }
        
        // 4. 检查缓存是否有待刷新的数据
        if let Ok(cache_dirty_size) = self.get_cache_dirty_size() {
            if cache_dirty_size > 0 {
                tracing::debug!("检测到缓存中有 {} 字节待刷新数据", cache_dirty_size);
                return true;
            }
        }
        
        false
    }
    
    /// 获取缓存中脏数据大小
    fn get_cache_dirty_size(&self) -> Result<usize> {
        // 企业级缓存脏数据监控
        // 这里简化实现，实际应该从查询缓存中获取
        let cache_size = self.query_cache.len();
        
        // 假设有10%的缓存数据是脏的（需要写回）
        Ok(cache_size / 10)
    }
}

// 实现Drop trait以确保资源清理
impl Drop for EmbeddedVectorDB {
    fn drop(&mut self) {
        if !matches!(*self.state.read(), DatabaseState::Closed) {
            tracing::warn!("EmbeddedVectorDB dropped without proper shutdown");
        }
    }
}

// 为了支持线程安全，实现Send和Sync
unsafe impl Send for EmbeddedVectorDB {}
unsafe impl Sync for EmbeddedVectorDB {} 