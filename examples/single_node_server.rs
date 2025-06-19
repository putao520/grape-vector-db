//! 单节点服务器完整示例
//! 
//! 这个示例展示了如何启动和管理 Grape Vector Database 的单节点服务，
//! 包括 gRPC 服务器、REST API、监控指标和客户端使用。

use grape_vector_db::{
    grpc::{start_grpc_server, VectorDbServiceImpl},
    rest::start_rest_server,
    config::{ServerConfig, VectorConfig, StorageConfig, IndexConfig, MonitoringConfig},
    VectorDatabase, VectorDbConfig,
    types::{Point, SearchRequest, Filter, Condition, FilterOperator},
    metrics::MetricsCollector,
    errors::Result,
};
use std::{
    sync::Arc,
    time::Duration,
    collections::HashMap,
};
use tokio::{
    sync::{RwLock, mpsc},
    time::sleep,
    signal,
};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};

/// 服务器配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrapeServerConfig {
    pub server: ServerConfig,
    pub vector: VectorConfig,
    pub storage: StorageConfig,
    pub index: IndexConfig,
    pub monitoring: MonitoringConfig,
}

impl Default for GrapeServerConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                grpc_port: 6334,
                rest_port: 6333,
                data_dir: "./data".into(),
                log_level: "info".to_string(),
                log_format: "pretty".to_string(),
                worker_threads: num_cpus::get(),
                max_connections: 1000,
                request_timeout_secs: 30,
            },
            vector: VectorConfig {
                dimension: 768,
                distance_metric: "cosine".to_string(),
                binary_quantization: true,
                quantization_threshold: 0.5,
            },
            storage: StorageConfig {
                compression_enabled: true,
                cache_size_mb: 512,
                write_buffer_size_mb: 64,
                max_write_buffer_number: 4,
                target_file_size_mb: 128,
                bloom_filter_bits_per_key: 10,
            },
            index: IndexConfig {
                m: 32,
                ef_construction: 400,
                ef_search: 200,
                max_m: 64,
                max_level: 16,
            },
            monitoring: MonitoringConfig {
                prometheus_enabled: true,
                prometheus_port: 9090,
                health_check_path: "/health".to_string(),
                metrics_interval_secs: 10,
            },
        }
    }
}

/// 单节点服务器
pub struct GrapeVectorDbServer {
    config: GrapeServerConfig,
    database: Arc<RwLock<VectorDatabase>>,
    metrics: Arc<MetricsCollector>,
    shutdown_tx: mpsc::UnboundedSender<()>,
    shutdown_rx: Option<mpsc::UnboundedReceiver<()>>,
}

impl GrapeVectorDbServer {
    /// 创建新的服务器实例
    pub async fn new(config: GrapeServerConfig) -> Result<Self> {
        info!("初始化 Grape Vector Database 服务器...");
        
        // 创建数据库配置
        let db_config = VectorDbConfig {
            vector_dimension: config.vector.dimension,
            distance_metric: config.vector.distance_metric.clone(),
            hnsw: grape_vector_db::config::HnswConfig {
                m: config.index.m,
                ef_construction: config.index.ef_construction,
                ef_search: config.index.ef_search,
                max_m: config.index.max_m,
                ml: 1.0 / (2.0_f32.ln()),
                ..Default::default()
            },
            cache: grape_vector_db::config::CacheConfig {
                embedding_cache_size: config.storage.cache_size_mb * 1024,
                query_cache_size: 5000,
                cache_ttl_seconds: 86400,
            },
            ..Default::default()
        };

        // 初始化数据库
        let database = VectorDatabase::with_config(&config.server.data_dir, db_config).await?;
        let database = Arc::new(RwLock::new(database));

        // 初始化指标收集器
        let metrics = Arc::new(MetricsCollector::new());

        // 创建关闭信号通道
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            database,
            metrics,
            shutdown_tx,
            shutdown_rx: Some(shutdown_rx),
        })
    }

    /// 启动服务器
    pub async fn start(&mut self) -> Result<()> {
        info!("启动 Grape Vector Database 服务器");
        info!("  gRPC 端口: {}", self.config.server.grpc_port);
        info!("  REST 端口: {}", self.config.server.rest_port);
        info!("  数据目录: {}", self.config.server.data_dir.display());

        // 启动监控指标服务
        if self.config.monitoring.prometheus_enabled {
            self.start_metrics_server().await?;
        }

        // 启动 gRPC 服务
        let grpc_handle = self.start_grpc_service().await?;

        // 启动 REST API 服务
        let rest_handle = self.start_rest_service().await?;

        // 启动监控任务
        let monitoring_handle = self.start_monitoring_task().await;

        info!("🚀 服务器启动完成，等待请求...");

        // 等待关闭信号
        self.wait_for_shutdown().await;

        // 优雅关闭
        info!("开始优雅关闭...");
        self.graceful_shutdown(grpc_handle, rest_handle, monitoring_handle).await?;

        Ok(())
    }

    /// 启动 gRPC 服务
    async fn start_grpc_service(&self) -> Result<tokio::task::JoinHandle<()>> {
        let addr = format!("0.0.0.0:{}", self.config.server.grpc_port).parse()?;
        let database = self.database.clone();
        let metrics = self.metrics.clone();

        let handle = tokio::spawn(async move {
            let service = VectorDbServiceImpl::new(database, metrics);
            
            if let Err(e) = start_grpc_server(service, addr).await {
                error!("gRPC 服务启动失败: {}", e);
            }
        });

        info!("gRPC 服务已启动在端口 {}", self.config.server.grpc_port);
        Ok(handle)
    }

    /// 启动 REST API 服务
    async fn start_rest_service(&self) -> Result<tokio::task::JoinHandle<()>> {
        let addr = format!("0.0.0.0:{}", self.config.server.rest_port);
        let database = self.database.clone();
        let metrics = self.metrics.clone();
        let health_path = self.config.monitoring.health_check_path.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = start_rest_server(database, metrics, &addr, &health_path).await {
                error!("REST API 服务启动失败: {}", e);
            }
        });

        info!("REST API 服务已启动在端口 {}", self.config.server.rest_port);
        Ok(handle)
    }

    /// 启动监控指标服务
    async fn start_metrics_server(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.monitoring.prometheus_port);
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let app = axum::Router::new()
                .route("/metrics", axum::routing::get({
                    let metrics = metrics.clone();
                    move || async move { metrics.export_prometheus() }
                }));

            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });

        info!("Prometheus 指标服务已启动在端口 {}", self.config.monitoring.prometheus_port);
        Ok(())
    }

    /// 启动监控任务
    async fn start_monitoring_task(&self) -> tokio::task::JoinHandle<()> {
        let database = self.database.clone();
        let metrics = self.metrics.clone();
        let interval_secs = self.config.monitoring.metrics_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                
                // 收集数据库统计信息
                if let Ok(db) = database.try_read() {
                    let stats = db.stats();
                    
                    // 更新指标
                    metrics.update_vector_count(stats.vector_count as f64);
                    metrics.update_memory_usage(stats.memory_usage_mb);
                    metrics.update_cache_hit_rate(stats.cache_hit_rate);
                    
                    debug!("指标更新: {} 个向量, {:.2} MB 内存", 
                           stats.vector_count, stats.memory_usage_mb);
                }
            }
        })
    }

    /// 等待关闭信号
    async fn wait_for_shutdown(&mut self) {
        let mut shutdown_rx = self.shutdown_rx.take().expect("shutdown_rx should be available");
        
        tokio::select! {
            _ = signal::ctrl_c() => {
                info!("收到 Ctrl+C 信号");
            }
            _ = shutdown_rx.recv() => {
                info!("收到关闭信号");
            }
        }
    }

    /// 优雅关闭
    async fn graceful_shutdown(
        &self,
        grpc_handle: tokio::task::JoinHandle<()>,
        rest_handle: tokio::task::JoinHandle<()>,
        monitoring_handle: tokio::task::JoinHandle<()>,
    ) -> Result<()> {
        // 停止接受新连接
        info!("停止接受新连接...");
        
        // 等待当前请求完成
        sleep(Duration::from_secs(2)).await;
        
        // 保存数据
        info!("保存数据...");
        if let Ok(db) = self.database.try_read() {
            db.save().await?;
        }
        
        // 终止服务任务
        grpc_handle.abort();
        rest_handle.abort();
        monitoring_handle.abort();
        
        info!("服务器已优雅关闭");
        Ok(())
    }

    /// 发送关闭信号
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database 单节点服务器启动");

    // 检查命令行参数
    let args: Vec<String> = std::env::args().collect();
    let config_path = args.get(1).unwrap_or(&"config.toml".to_string());

    // 加载配置
    let config = load_config(config_path).await?;
    info!("配置加载完成: {}", config_path);

    // 运行示例演示
    run_demo().await?;

    // 创建并启动服务器
    let mut server = GrapeVectorDbServer::new(config).await?;
    server.start().await?;

    Ok(())
}

/// 加载配置文件
async fn load_config(config_path: &str) -> Result<GrapeServerConfig> {
    if std::path::Path::new(config_path).exists() {
        let content = tokio::fs::read_to_string(config_path).await?;
        let config: GrapeServerConfig = toml::from_str(&content)?;
        Ok(config)
    } else {
        info!("配置文件不存在，使用默认配置: {}", config_path);
        let config = GrapeServerConfig::default();
        
        // 保存默认配置到文件
        let content = toml::to_string_pretty(&config)?;
        tokio::fs::write(config_path, content).await?;
        info!("默认配置已保存到: {}", config_path);
        
        Ok(config)
    }
}

/// 运行演示
async fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
    info!("🎯 运行客户端演示...");

    // 等待服务器启动
    sleep(Duration::from_secs(1)).await;

    // 演示 REST API 客户端
    demo_rest_client().await?;

    // 演示 gRPC 客户端
    // demo_grpc_client().await?;

    info!("✅ 演示完成");
    Ok(())
}

/// REST API 客户端演示
async fn demo_rest_client() -> Result<(), Box<dyn std::error::Error>> {
    info!("📡 REST API 客户端演示");

    let client = reqwest::Client::new();
    let base_url = "http://localhost:6333";

    // 1. 健康检查
    let health_response = client
        .get(&format!("{}/health", base_url))
        .send()
        .await?;
    
    info!("健康检查: {}", health_response.status());

    // 2. 添加向量
    let vector_data = serde_json::json!({
        "id": "demo_doc_1",
        "vector": generate_random_vector(768),
        "payload": {
            "title": "演示文档1",
            "category": "示例",
            "language": "zh"
        }
    });

    let add_response = client
        .post(&format!("{}/vectors", base_url))
        .header("Content-Type", "application/json")
        .json(&vector_data)
        .send()
        .await?;

    info!("添加向量: {}", add_response.status());

    // 3. 批量添加向量
    let batch_data = serde_json::json!({
        "vectors": [
            {
                "id": "demo_doc_2",
                "vector": generate_random_vector(768),
                "payload": {"title": "演示文档2", "category": "批量"}
            },
            {
                "id": "demo_doc_3", 
                "vector": generate_random_vector(768),
                "payload": {"title": "演示文档3", "category": "批量"}
            }
        ]
    });

    let batch_response = client
        .post(&format!("{}/vectors/batch", base_url))
        .header("Content-Type", "application/json")
        .json(&batch_data)
        .send()
        .await?;

    info!("批量添加: {}", batch_response.status());

    // 4. 搜索向量
    let search_data = serde_json::json!({
        "vector": generate_random_vector(768),
        "limit": 5,
        "filter": {
            "conditions": [
                {
                    "field": "category",
                    "operator": "eq", 
                    "value": "示例"
                }
            ]
        }
    });

    let search_response = client
        .post(&format!("{}/vectors/search", base_url))
        .header("Content-Type", "application/json")
        .json(&search_data)
        .send()
        .await?;

    if search_response.status().is_success() {
        let results: serde_json::Value = search_response.json().await?;
        info!("搜索结果: {} 个", results["results"].as_array().unwrap_or(&vec![]).len());
    }

    // 5. 获取统计信息
    let stats_response = client
        .get(&format!("{}/stats", base_url))
        .send()
        .await?;

    if stats_response.status().is_success() {
        let stats: serde_json::Value = stats_response.json().await?;
        info!("数据库统计: {}", stats);
    }

    Ok(())
}

/// 生成随机向量
fn generate_random_vector(dimension: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dimension).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

// 为了编译通过，我们需要定义一些配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub grpc_port: u16,
    pub rest_port: u16,
    pub data_dir: std::path::PathBuf,
    pub log_level: String,
    pub log_format: String,
    pub worker_threads: usize,
    pub max_connections: usize,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    pub dimension: usize,
    pub distance_metric: String,
    pub binary_quantization: bool,
    pub quantization_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub compression_enabled: bool,
    pub cache_size_mb: usize,
    pub write_buffer_size_mb: usize,
    pub max_write_buffer_number: usize,
    pub target_file_size_mb: usize,
    pub bloom_filter_bits_per_key: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
    pub max_m: usize,
    pub max_level: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub health_check_path: String,
    pub metrics_interval_secs: u64,
}

// 为了演示目的的简化实现
mod simplified_impls {
    use super::*;

    pub async fn start_grpc_server<T>(_service: T, _addr: std::net::SocketAddr) -> Result<()> {
        // 简化的 gRPC 服务器启动
        info!("gRPC 服务器模拟启动");
        Ok(())
    }

    pub async fn start_rest_server<T>(_db: T, _metrics: Arc<MetricsCollector>, _addr: &str, _health_path: &str) -> Result<()> {
        // 简化的 REST 服务器启动
        info!("REST 服务器模拟启动");
        Ok(())
    }
}

use simplified_impls::*;