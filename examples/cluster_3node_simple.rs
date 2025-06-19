//! 3节点集群基础示例
//! 
//! 这个示例展示了如何模拟一个3节点集群的基本概念和数据分布。

use grape_vector_db::*;
use std::{collections::HashMap, time::Duration};
use tokio::{signal, time::sleep};
use tracing::{info, warn, error};

/// 集群节点配置
#[derive(Debug, Clone)]
pub struct ClusterNodeConfig {
    pub node_id: String,
    pub data_dir: String,
    pub port: u16,
    pub is_leader: bool,
}

/// 简化的集群节点
pub struct ClusterNode {
    pub config: ClusterNodeConfig,
    pub database: VectorDatabase,
    pub node_status: NodeStatus,
}

#[derive(Debug, Clone)]
pub enum NodeStatus {
    Leader,
    Follower,
    Candidate,
    Offline,
}

impl ClusterNode {
    /// 创建新的集群节点
    pub async fn new(config: ClusterNodeConfig) -> Result<Self, Box<dyn std::error::Error>> {
        info!("初始化集群节点: {}", config.node_id);

        // 创建数据目录
        tokio::fs::create_dir_all(&config.data_dir).await?;

        // 初始化数据库
        let database = VectorDatabase::new(&config.data_dir).await?;

        let node_status = if config.is_leader {
            NodeStatus::Leader
        } else {
            NodeStatus::Follower
        };

        Ok(Self {
            config,
            database,
            node_status,
        })
    }

    /// 启动节点
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("启动节点: {} (状态: {:?})", self.config.node_id, self.node_status);

        // 根据节点类型加载不同的示例数据
        self.load_node_specific_data().await?;

        Ok(())
    }

    /// 加载节点特定的数据
    async fn load_node_specific_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let node_id = &self.config.node_id;
        
        match self.node_status {
            NodeStatus::Leader => {
                info!("领导节点 {} 加载核心数据", node_id);
                self.load_leader_data().await?;
            }
            NodeStatus::Follower => {
                info!("跟随节点 {} 加载分片数据", node_id);
                self.load_follower_data().await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// 领导节点加载数据
    async fn load_leader_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let docs = vec![
            Document {
                id: "leader_doc_1".to_string(),
                content: "领导节点负责协调集群操作和数据一致性。".to_string(),
                title: Some("领导节点职责".to_string()),
                language: Some("zh".to_string()),
                package_name: Some("cluster-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("leadership".to_string()),
                metadata: [
                    ("node_type".to_string(), "leader".to_string()),
                    ("shard".to_string(), "0-5".to_string()),
                ].into(),
            },
            Document {
                id: "leader_doc_2".to_string(),
                content: "Raft算法确保分布式系统的强一致性。".to_string(),
                title: Some("Raft共识算法".to_string()),
                language: Some("zh".to_string()),
                package_name: Some("consensus-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("algorithm".to_string()),
                metadata: [
                    ("node_type".to_string(), "leader".to_string()),
                    ("algorithm".to_string(), "raft".to_string()),
                ].into(),
            },
        ];

        self.database.add_documents(docs).await?;
        Ok(())
    }

    /// 跟随节点加载数据
    async fn load_follower_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let node_id = &self.config.node_id;
        let shard_range = if node_id == "node-2" { "6-11" } else { "12-16" };

        let docs = vec![
            Document {
                id: format!("{}_doc_1", node_id),
                content: format!("跟随节点{}处理分片{}的数据。", node_id, shard_range),
                title: Some(format!("节点{}数据分片", node_id)),
                language: Some("zh".to_string()),
                package_name: Some("shard-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("sharding".to_string()),
                metadata: [
                    ("node_type".to_string(), "follower".to_string()),
                    ("node_id".to_string(), node_id.clone()),
                    ("shard".to_string(), shard_range.to_string()),
                ].into(),
            },
            Document {
                id: format!("{}_doc_2", node_id),
                content: format!("数据复制确保{}节点的高可用性。", node_id),
                title: Some(format!("节点{}数据复制", node_id)),
                language: Some("zh".to_string()),
                package_name: Some("replication-docs".to_string()),
                version: Some("1.0".to_string()),
                doc_type: Some("replication".to_string()),
                metadata: [
                    ("node_type".to_string(), "follower".to_string()),
                    ("feature".to_string(), "replication".to_string()),
                ].into(),
            },
        ];

        self.database.add_documents(docs).await?;
        Ok(())
    }

    /// 获取节点统计信息
    pub fn get_stats(&self) -> DatabaseStats {
        self.database.stats()
    }

    /// 搜索数据
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        self.database.search(query, limit).await
    }
}

/// 简化的3节点集群
pub struct SimpleCluster {
    pub nodes: Vec<ClusterNode>,
}

impl SimpleCluster {
    /// 创建新的3节点集群
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        info!("初始化3节点集群");

        let node_configs = vec![
            ClusterNodeConfig {
                node_id: "node-1".to_string(),
                data_dir: "./cluster_data/node1".to_string(),
                port: 7001,
                is_leader: true,
            },
            ClusterNodeConfig {
                node_id: "node-2".to_string(),
                data_dir: "./cluster_data/node2".to_string(),
                port: 7002,
                is_leader: false,
            },
            ClusterNodeConfig {
                node_id: "node-3".to_string(),
                data_dir: "./cluster_data/node3".to_string(),
                port: 7003,
                is_leader: false,
            },
        ];

        let mut nodes = Vec::new();
        for config in node_configs {
            let node = ClusterNode::new(config).await?;
            nodes.push(node);
        }

        Ok(Self { nodes })
    }

    /// 启动整个集群
    pub async fn start_cluster(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🚀 启动3节点集群");

        // 启动所有节点
        for node in &mut self.nodes {
            node.start().await?;
            sleep(Duration::from_secs(1)).await; // 模拟启动间隔
        }

        info!("✅ 集群启动完成");
        self.verify_cluster_health().await?;

        Ok(())
    }

    /// 验证集群健康状态
    async fn verify_cluster_health(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🔍 验证集群健康状态");

        for node in &self.nodes {
            let stats = node.get_stats();
            info!("节点 {} 状态:", node.config.node_id);
            info!("  文档数量: {}", stats.document_count);
            info!("  向量数量: {}", stats.vector_count);
            info!("  状态: {:?}", node.node_status);
        }

        info!("✅ 集群健康检查完成");
        Ok(())
    }

    /// 演示分布式搜索
    pub async fn demo_distributed_search(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🔍 演示分布式搜索");

        let queries = vec!["集群", "数据", "节点", "算法"];

        for query in queries {
            info!("=== 搜索查询: '{}' ===", query);
            
            let mut all_results = Vec::new();
            
            // 在每个节点上执行搜索
            for node in &self.nodes {
                let results = node.search(query, 3).await?;
                info!("节点 {} 返回 {} 个结果", node.config.node_id, results.len());
                
                for result in results {
                    all_results.push((node.config.node_id.clone(), result));
                }
            }

            // 显示合并后的结果
            info!("总共找到 {} 个结果:", all_results.len());
            for (node_id, result) in all_results.iter().take(5) {
                info!("  [{}] {}: {:.4}", node_id, result.title, result.similarity_score);
            }
        }

        Ok(())
    }

    /// 演示故障转移
    pub async fn demo_failover(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🔄 演示故障转移");

        // 找到当前领导者
        let leader_idx = self.nodes.iter()
            .position(|n| matches!(n.node_status, NodeStatus::Leader))
            .unwrap();

        info!("当前领导者: {}", self.nodes[leader_idx].config.node_id);

        // 模拟领导者故障
        info!("模拟领导者故障...");
        self.nodes[leader_idx].node_status = NodeStatus::Offline;

        // 选举新领导者 (简化版本)
        if let Some(new_leader) = self.nodes.iter_mut()
            .find(|n| matches!(n.node_status, NodeStatus::Follower)) 
        {
            new_leader.node_status = NodeStatus::Leader;
            info!("新领导者当选: {}", new_leader.config.node_id);
        }

        // 验证集群仍然可用
        info!("验证集群可用性...");
        self.verify_cluster_health().await?;

        info!("✅ 故障转移演示完成");
        Ok(())
    }

    /// 关闭集群
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("🛑 关闭集群");

        for node in &mut self.nodes {
            info!("保存节点 {} 的数据", node.config.node_id);
            node.database.save().await?;
        }

        info!("✅ 集群已关闭");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🍇 Grape Vector Database 3节点集群示例");

    // 创建并启动集群
    let mut cluster = SimpleCluster::new().await?;
    cluster.start_cluster().await?;

    // 运行演示
    cluster.demo_distributed_search().await?;
    cluster.demo_failover().await?;

    // 保持运行状态，等待手动中断
    info!("集群演示完成，按 Ctrl+C 继续...");
    match signal::ctrl_c().await {
        Ok(()) => info!("收到 Ctrl+C 信号"),
        Err(err) => error!("监听信号失败: {}", err),
    }

    // 关闭集群
    cluster.shutdown().await?;

    Ok(())
}