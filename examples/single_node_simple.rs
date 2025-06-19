//! 单节点服务器基础示例
//! 
//! 这个示例展示了如何创建一个基础的单节点向量数据库服务器。

use grape_vector_db::*;
use std::time::Duration;
use tokio::{signal, time::sleep};
use tracing::{info, error};

/// 简化的服务器配置
#[derive(Debug, Clone)]
pub struct SimpleServerConfig {
    pub data_dir: String,
    pub port: u16,
    pub log_level: String,
}

impl Default for SimpleServerConfig {
    fn default() -> Self {
        Self {
            data_dir: "./server_data".to_string(),
            port: 8080,
            log_level: "info".to_string(),
        }
    }
}

/// 简化的向量数据库服务器
pub struct SimpleVectorDbServer {
    config: SimpleServerConfig,
    database: VectorDatabase,
}

impl SimpleVectorDbServer {
    /// 创建新的服务器实例
    pub async fn new(config: SimpleServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        info!("初始化 Grape Vector Database 服务器...");
        
        // 创建数据目录
        tokio::fs::create_dir_all(&config.data_dir).await?;

        // 初始化数据库
        let mut db_config = VectorDbConfig::default();
        db_config.db_path = config.data_dir.clone();
        let database = VectorDatabase::new(db_config.db_path.clone().into(), db_config).await?;

        Ok(Self {
            config,
            database,
        })
    }

    /// 启动服务器
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🚀 启动 Grape Vector Database 服务器");
        info!("  数据目录: {}", self.config.data_dir);
        info!("  端口: {}", self.config.port);

        // 预加载一些示例数据
        self.load_sample_data().await?;

        // 启动HTTP服务 (简化版本)
        let server_handle = self.start_http_server().await;

        info!("✅ 服务器启动完成，等待请求...");
        info!("   访问 http://localhost:{}/health 检查健康状态", self.config.port);

        // 等待关闭信号
        self.wait_for_shutdown().await;

        // 优雅关闭
        info!("开始优雅关闭...");
        server_handle.abort();
        // 删除save方法调用，因为数据库会自动持久化
        info!("服务器已关闭");

        Ok(())
    }

    /// 加载示例数据
    async fn load_sample_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("加载示例数据...");

        let sample_docs = vec![
            Document {
                id: "server_doc_1".to_string(),
                content: "Grape Vector Database 是一个高性能的向量数据库系统。".to_string(),
                title: Some("Grape Vector Database 介绍".to_string()),
                language: Some("zh".to_string()),
                package_name: Some("grape-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("documentation".to_string()),
                vector: None,
                metadata: [
                    ("category".to_string(), "技术文档".to_string()),
                    ("tags".to_string(), "数据库,向量搜索".to_string()),
                ].into(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            Document {
                id: "server_doc_2".to_string(),
                content: "REST API 提供了标准的HTTP接口，方便客户端集成。".to_string(),
                title: Some("REST API 使用指南".to_string()),
                language: Some("zh".to_string()),
                package_name: Some("api-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("guide".to_string()),
                vector: None,
                metadata: [
                    ("category".to_string(), "API文档".to_string()),
                    ("tags".to_string(), "REST,HTTP,API".to_string()),
                ].into(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            Document {
                id: "server_doc_3".to_string(),
                content: "单节点部署适合中小规模应用，提供稳定的向量搜索服务。".to_string(),
                title: Some("单节点部署指南".to_string()),
                language: Some("zh".to_string()),
                package_name: Some("deployment-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("guide".to_string()),
                vector: None,
                metadata: [
                    ("category".to_string(), "部署文档".to_string()),
                    ("tags".to_string(), "部署,单节点".to_string()),
                ].into(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        ];

        for doc in sample_docs {
            self.database.add_document(doc).await?;
        }

        let stats = self.database.get_stats().await;
        info!("示例数据加载完成: {} 个文档", stats.document_count);

        Ok(())
    }

    /// 启动HTTP服务器 (简化版本)
    async fn start_http_server(&self) -> tokio::task::JoinHandle<()> {
        let port = self.config.port;
        
        tokio::spawn(async move {
            info!("HTTP 服务器模拟启动在端口 {}", port);
            
            // 模拟服务器运行
            loop {
                sleep(Duration::from_secs(10)).await;
                info!("服务器运行中... (端口: {})", port);
            }
        })
    }

    /// 等待关闭信号
    async fn wait_for_shutdown(&self) {
        match signal::ctrl_c().await {
            Ok(()) => info!("收到 Ctrl+C 信号"),
            Err(err) => error!("监听关闭信号失败: {}", err),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database 单节点服务器示例");

    // 加载配置
    let config = SimpleServerConfig::default();
    info!("配置: {:?}", config);

    // 运行客户端演示
    run_client_demo().await?;

    // 创建并启动服务器
    let mut server = SimpleVectorDbServer::new(config).await?;
    server.start().await?;

    Ok(())
}

/// 运行客户端演示
async fn run_client_demo() -> Result<(), Box<dyn std::error::Error>> {
    info!("🎯 运行客户端演示...");

    // 创建一个临时数据库来模拟客户端操作
    let client_config = VectorDbConfig::default();
    let client_db = VectorDatabase::new("./client_demo_data".into(), client_config).await?;

    // 模拟客户端添加数据
    let client_docs = vec![
        Document {
            id: "client_doc_1".to_string(),
            content: "客户端可以通过HTTP API与服务器通信。".to_string(),
            title: Some("客户端通信".to_string()),
            language: Some("zh".to_string()),
            package_name: Some("client-demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("example".to_string()),
            vector: None,
            metadata: [("source".to_string(), "client".to_string())].into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        Document {
            id: "client_doc_2".to_string(),
            content: "JSON格式的请求和响应使集成变得简单。".to_string(),
            title: Some("JSON API".to_string()),
            language: Some("zh".to_string()),
            package_name: Some("client-demo".to_string()),
            version: Some("1.0".to_string()),
            doc_type: Some("example".to_string()),
            vector: None,
            metadata: [("source".to_string(), "client".to_string())].into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
    ];

    // 添加文档 - 使用单个文档添加方法
    for doc in client_docs {
        client_db.add_document(doc).await?;
    }

    // 执行搜索
    let search_results = client_db.text_search("API通信", 5).await?;
    info!("客户端搜索结果: {} 个", search_results.len());
    
    for result in search_results {
        info!("  - {}: {:.4}", result.document.title, result.score);
    }

    info!("✅ 客户端演示完成");
    Ok(())
}