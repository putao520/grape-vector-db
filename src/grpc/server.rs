use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{info, error, warn};
use std::collections::HashMap;

use crate::{
    VectorDatabase, VectorDbConfig,
    types::{SearchRequest as InternalSearchRequest, InternalSearchResult, GrpcSearchResponse},
    performance::PerformanceStats,
    distributed::{raft::RaftNode, cluster::ClusterManager, shard::ShardManager},
    errors::Result as DbResult,
};

use super::{
    vector_db_service_server::{VectorDbService, VectorDbServiceServer},
    types::*,
    *,
};

/// gRPC服务实现
pub struct VectorDbServiceImpl {
    /// 向量数据库实例
    database: Arc<RwLock<VectorDatabase>>,
    /// 向量索引 (直接访问以提高性能)
    vector_index: Arc<tokio::sync::RwLock<dyn crate::index::VectorIndex>>,
    /// Raft节点
    raft_node: Arc<RaftNode>,
    /// 集群管理器
    cluster_manager: Arc<ClusterManager>,
    /// 分片管理器
    shard_manager: Arc<ShardManager>,
}

impl VectorDbServiceImpl {
    pub fn new(
        database: Arc<RwLock<VectorDatabase>>,
        vector_index: Arc<tokio::sync::RwLock<dyn crate::index::VectorIndex>>,
        raft_node: Arc<RaftNode>,
        cluster_manager: Arc<ClusterManager>,
        shard_manager: Arc<ShardManager>,
    ) -> Self {
        Self {
            database,
            vector_index,
            raft_node,
            cluster_manager,
            shard_manager,
        }
    }
}

#[tonic::async_trait]
impl VectorDbService for VectorDbServiceImpl {
    /// 添加文档
    async fn add_document(
        &self,
        request: Request<AddDocumentRequest>,
    ) -> Result<Response<AddDocumentResponse>, Status> {
        let req = request.into_inner();
        
        let doc = req.document.ok_or_else(|| {
            Status::invalid_argument("Missing document in request")
        })?;

        let start_time = Instant::now();
        
        // 转换protobuf Document到内部Document
        let internal_doc = doc.into();

        // 添加文档
        match self.database.write().await.add_document(internal_doc).await {
            Ok(doc_id) => {
                let elapsed = start_time.elapsed();
                info!("添加文档成功: {} (耗时: {:?})", doc_id, elapsed);
                
                let response = AddDocumentResponse {
                    document_id: doc_id,
                    error: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("添加文档失败: {}", e);
                let response = AddDocumentResponse {
                    document_id: String::new(),
                    error: Some(format!("添加文档失败: {}", e)),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// 搜索文档
    async fn search_documents(
        &self,
        request: Request<SearchDocumentRequest>,
    ) -> Result<Response<SearchDocumentResponse>, Status> {
        let req = request.into_inner();
        let start_time = Instant::now();

        // 构建搜索请求
        let search_request = req.try_into()
            .map_err(|e: String| Status::invalid_argument(e))?;

        // 执行搜索
        match self.database.read().await.search_documents(search_request).await {
            Ok(results) => {
                let elapsed = start_time.elapsed();
                
                // 转换结果格式
                let grpc_results: Vec<SearchResult> = results.results.into_iter().map(|r| {
                    SearchResult {
                        document_id: r.document_id,
                        title: r.title.unwrap_or_default(),
                        content_snippet: r.content_snippet,
                        similarity_score: r.similarity_score,
                        package_name: r.package_name.unwrap_or_default(),
                        doc_type: r.doc_type.unwrap_or_default(),
                        metadata: r.metadata,
                    }
                }).collect();

                info!("搜索完成: 找到{}个结果 (耗时: {:?})", grpc_results.len(), elapsed);

                let response = SearchDocumentResponse {
                    results: grpc_results,
                    query_time_ms: elapsed.as_millis() as f64,
                    total_matches: results.total_matches as u32,
                    error: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("搜索失败: {}", e);
                let response = SearchDocumentResponse {
                    results: vec![],
                    query_time_ms: 0.0,
                    total_matches: 0,
                    error: Some(format!("搜索失败: {}", e)),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// 获取文档
    async fn get_document(
        &self,
        request: Request<GetDocumentRequest>,
    ) -> Result<Response<GetDocumentResponse>, Status> {
        let req = request.into_inner();
        
        match self.database.read().await.get_document(&req.document_id).await {
            Ok(Some(doc)) => {
                let grpc_doc = doc.into();
                
                let response = GetDocumentResponse {
                    document: Some(grpc_doc),
                    error: None,
                };
                Ok(Response::new(response))
            }
            Ok(None) => {
                let response = GetDocumentResponse {
                    document: None,
                    error: Some("文档未找到".to_string()),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("获取文档失败: {}", e);
                let response = GetDocumentResponse {
                    document: None,
                    error: Some(format!("获取文档失败: {}", e)),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// 删除文档
    async fn delete_document(
        &self,
        request: Request<DeleteDocumentRequest>,
    ) -> Result<Response<DeleteDocumentResponse>, Status> {
        let req = request.into_inner();
        
        match self.database.write().await.delete_document(&req.document_id).await {
            Ok(deleted) => {
                info!("删除文档: {} (删除: {})", req.document_id, deleted);
                
                let response = DeleteDocumentResponse {
                    deleted,
                    error: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("删除文档失败: {}", e);
                let response = DeleteDocumentResponse {
                    deleted: false,
                    error: Some(format!("删除文档失败: {}", e)),
                };
                Ok(Response::new(response))
            }
        }
    }

    /// 获取统计信息
    async fn get_stats(
        &self,
        _request: Request<GetStatsRequest>,
    ) -> Result<Response<GetStatsResponse>, Status> {
        // 同步获取统计信息，不需要await
        let stats = self.database.read().await.get_stats();
        
        let grpc_stats = DatabaseStats {
            document_count: stats.document_count,
            dense_vector_count: stats.dense_vector_count,
            sparse_vector_count: stats.sparse_vector_count,
            memory_usage_mb: stats.memory_usage_mb,
            dense_index_size_mb: stats.dense_index_size_mb,
            sparse_index_size_mb: stats.sparse_index_size_mb,
            cache_hit_rate: stats.cache_hit_rate,
        };
        
        let response = GetStatsResponse {
            stats: Some(grpc_stats),
            error: None,
        };
        Ok(Response::new(response))
    }

    /// 获取性能指标
    async fn get_metrics(
        &self,
        _request: Request<GetMetricsRequest>,
    ) -> Result<Response<GetMetricsResponse>, Status> {
        // 同步获取性能指标，不需要await
        let stats = self.database.read().await.get_performance_metrics();
        
        let grpc_metrics = PerformanceMetrics {
            avg_query_time_ms: stats.average_query_time_ms,
            p95_query_time_ms: stats.average_query_time_ms * 1.5, // 估算
            p99_query_time_ms: stats.average_query_time_ms * 2.0, // 估算
            total_queries: stats.total_queries,
            queries_per_second: if stats.average_query_time_ms > 0.0 { 
                1000.0 / stats.average_query_time_ms 
            } else { 
                0.0 
            },
            cache_hits: (stats.cache_hit_rate * stats.total_queries as f64) as u64,
            cache_misses: ((1.0 - stats.cache_hit_rate) * stats.total_queries as f64) as u64,
            memory_usage_mb: stats.memory_usage_bytes as f64 / (1024.0 * 1024.0),
        };
        
        let response = GetMetricsResponse {
            metrics: Some(grpc_metrics),
            error: None,
        };
        Ok(Response::new(response))
    }


    // 向量操作方法
    async fn upsert_vector(&self, request: Request<UpsertVectorRequest>) -> Result<Response<UpsertVectorResponse>, Status> {
        let req = request.into_inner();
        
        // 验证请求
        let point = req.point.ok_or_else(|| {
            Status::invalid_argument("Missing point in request")
        })?;
        
        let vector = point.vector.ok_or_else(|| {
            Status::invalid_argument("Missing vector in point")
        })?;
        
        if vector.values.is_empty() {
            return Err(Status::invalid_argument("Vector values cannot be empty"));
        }
        
        // 创建文档记录
        let doc = crate::types::Document {
            id: point.id.clone(),
            title: None,
            content: String::new(), // 向量操作可能没有文本内容
            language: None,
            version: None,
            doc_type: None,
            package_name: None,
            vector: Some(vector.values),
            metadata: point.payload,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        
        // 插入或更新文档
        match self.database.write().await.add_document(doc).await {
            Ok(_) => {
                let response = UpsertVectorResponse {
                    success: true,

                    error: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => {

                error!("向量插入失败: {}", e);
                let response = UpsertVectorResponse {
                    success: false,
                    error: Some(format!("向量插入失败: {}", e)),

                };
                Ok(Response::new(response))
            }
        }
    }


    async fn delete_vector(&self, request: Request<DeleteVectorRequest>) -> Result<Response<DeleteVectorResponse>, Status> {
        let req = request.into_inner();
        
        match self.database.write().await.delete_document(&req.id).await {
            Ok(deleted) => {
                info!("删除向量: {} (删除: {})", req.id, deleted);
                
                let response = DeleteVectorResponse {
                    deleted,
                    error: None,

                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("删除向量失败: {}", e);
                let response = DeleteVectorResponse {

                    deleted: false,
                    error: Some(format!("删除向量失败: {}", e)),

                };
                Ok(Response::new(response))
            }
        }
    }


    async fn search_vectors(&self, request: Request<SearchVectorRequest>) -> Result<Response<SearchVectorResponse>, Status> {
        let req = request.into_inner();
        
        let vector = req.vector.ok_or_else(|| {
            Status::invalid_argument("Missing vector in request")
        })?;
        
        if vector.values.is_empty() {
            return Err(Status::invalid_argument("Vector values cannot be empty"));
        }
        
        let start_time = Instant::now();
        
        // 执行向量搜索
        let vector_index = self.vector_index.read().await;
        
        match vector_index.search(&vector.values, req.limit as usize) {
            Ok(search_results) => {
                let elapsed = start_time.elapsed();
                
                // 转换结果格式
                let grpc_results: Vec<VectorSearchResult> = search_results.into_iter().map(|(id, score)| {
                    VectorSearchResult {
                        id,
                        score,
                        vector: None, // 为了性能，默认不返回向量
                        payload: std::collections::HashMap::new(),
                    }
                }).collect();

                info!("向量搜索完成: 找到{}个结果 (耗时: {:?})", grpc_results.len(), elapsed);

                let response = SearchVectorResponse {
                    results: grpc_results,
                    query_time_ms: elapsed.as_millis() as f64,

                    error: None,
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("向量搜索失败: {}", e);
                let response = SearchVectorResponse {
                    results: vec![],
                    query_time_ms: 0.0,
                    error: Some(format!("向量搜索失败: {}", e)),

                };
                Ok(Response::new(response))
            }
        }
    }


    async fn get_vector(&self, request: Request<GetVectorRequest>) -> Result<Response<GetVectorResponse>, Status> {
        let req = request.into_inner();
        
        match self.database.read().await.get_document(&req.id).await {
            Ok(Some(doc)) => {
                if let Some(vector_values) = doc.vector {
                    let vector = Vector {
                        values: vector_values,
                    };
                    
                    let point = Point {
                        id: doc.id,
                        vector: Some(vector),
                        payload: doc.metadata,
                    };
                    
                    let response = GetVectorResponse {
                        point: Some(point),
                        error: None,
                    };
                    Ok(Response::new(response))
                } else {
                    let response = GetVectorResponse {
                        point: None,
                        error: Some("文档没有向量数据".to_string()),
                    };
                    Ok(Response::new(response))
                }
            }
            Ok(None) => {
                let response = GetVectorResponse {
                    point: None,
                    error: Some("向量未找到".to_string()),

                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("获取向量失败: {}", e);
                let response = GetVectorResponse {

                    point: None,
                    error: Some(format!("获取向量失败: {}", e)),

                };
                Ok(Response::new(response))
            }
        }
    }

    // 集群管理方法
    async fn join_cluster(&self, request: Request<JoinClusterRequest>) -> Result<Response<JoinClusterResponse>, Status> {
        let req = request.into_inner();
        
        info!("节点请求加入集群: {}", req.node_id);
        
        // 这里应该调用集群管理器的方法
        // 暂时返回简单的成功响应
        let response = JoinClusterResponse {
            success: true,
            cluster_id: "default_cluster".to_string(),
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn leave_cluster(&self, request: Request<LeaveClusterRequest>) -> Result<Response<LeaveClusterResponse>, Status> {
        let req = request.into_inner();
        
        info!("节点请求离开集群: {}", req.node_id);
        
        // 这里应该调用集群管理器的方法
        // 暂时返回简单的成功响应
        let response = LeaveClusterResponse {
            success: true,
            error: None,
        };
        Ok(Response::new(response))

    }

    async fn get_cluster_info(&self, _request: Request<GetClusterInfoRequest>) -> Result<Response<GetClusterInfoResponse>, Status> {
        // 获取集群信息
        // 这里应该从集群管理器获取实际信息
        // 暂时返回模拟数据
        let response = GetClusterInfoResponse {
            cluster_id: "default_cluster".to_string(),
            leader_id: "node1".to_string(),
            members: vec!["node1".to_string()],
            term: 1,
            error: None,
        };
        Ok(Response::new(response))
    }

    // Raft 协议方法
    async fn heartbeat(&self, request: Request<HeartbeatRequest>) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        
        // 处理心跳请求
        // 这里应该调用Raft节点的heartbeat方法
        let response = HeartbeatResponse {
            success: true,
            term: req.term,
            leader_id: req.leader_id.clone(),
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesResponse>, Status> {
        let req = request.into_inner();
        
        info!("接收到AppendEntries请求: term={}, leader={}", req.term, req.leader_id);
        
        // 处理日志追加请求
        // 这里应该调用Raft节点的append_entries方法
        let response = AppendEntriesResponse {
            success: true,
            term: req.term,
            log_index: req.prev_log_index + req.entries.len() as u64,
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteResponse>, Status> {
        let req = request.into_inner();
        
        info!("接收到RequestVote请求: term={}, candidate={}", req.term, req.candidate_id);
        
        // 处理投票请求
        // 这里应该调用Raft节点的request_vote方法
        let response = RequestVoteResponse {
            vote_granted: true,
            term: req.term,
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn install_snapshot(&self, request: Request<InstallSnapshotRequest>) -> Result<Response<InstallSnapshotResponse>, Status> {
        let req = request.into_inner();
        
        info!("接收到InstallSnapshot请求: term={}, leader={}", req.term, req.leader_id);
        
        // 处理快照安装请求
        // 这里应该调用Raft节点的install_snapshot方法
        let response = InstallSnapshotResponse {
            success: true,
            term: req.term,
            error: None,
        };
        Ok(Response::new(response))
    }

    // 分片管理方法
    async fn migrate_shard(&self, request: Request<MigrateShardRequest>) -> Result<Response<MigrateShardResponse>, Status> {
        let req = request.into_inner();
        
        info!("接收到分片迁移请求: shard_id={}, target_node={}", req.shard_id, req.target_node);
        
        // 处理分片迁移请求
        // 这里应该调用分片管理器的migrate_shard方法
        let response = MigrateShardResponse {
            success: true,
            migration_id: format!("migration_{}", uuid::Uuid::new_v4()),
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn rebalance_shards(&self, _request: Request<RebalanceShardsRequest>) -> Result<Response<RebalanceShardsResponse>, Status> {
        info!("接收到分片重平衡请求");
        
        // 处理分片重平衡请求
        // 这里应该调用分片管理器的rebalance_shards方法
        let response = RebalanceShardsResponse {
            success: true,
            rebalance_id: format!("rebalance_{}", uuid::Uuid::new_v4()),
            affected_shards: vec![], // 实际实现中应该返回受影响的分片列表
            error: None,
        };
        Ok(Response::new(response))
    }

    async fn get_shard_info(&self, request: Request<GetShardInfoRequest>) -> Result<Response<GetShardInfoResponse>, Status> {
        let req = request.into_inner();
        
        // 获取分片信息
        // 这里应该从分片管理器获取实际信息
        let response = GetShardInfoResponse {
            shard_id: req.shard_id.clone(),
            node_id: "current_node".to_string(),
            status: "active".to_string(),
            document_count: 0,
            size_bytes: 0,
            error: None,
        };
        Ok(Response::new(response))
    }
}

/// 启动gRPC服务器
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    database: Arc<RwLock<VectorDatabase>>,
    vector_index: Arc<tokio::sync::RwLock<dyn crate::index::VectorIndex>>,
    raft_node: Arc<RaftNode>,
    cluster_manager: Arc<ClusterManager>,
    shard_manager: Arc<ShardManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = VectorDbServiceImpl::new(database, vector_index, raft_node, cluster_manager, shard_manager);
    
    info!("启动gRPC服务器，监听地址: {}", addr);

    Server::builder()
        .add_service(VectorDbServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
} 