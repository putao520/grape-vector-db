use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{info, error, warn};
use std::collections::HashMap;

use crate::{
    VectorDatabase, VectorDbConfig,
    types::{SearchRequest as InternalSearchRequest, SearchResult as InternalSearchResult},
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
        raft_node: Arc<RaftNode>,
        cluster_manager: Arc<ClusterManager>,
        shard_manager: Arc<ShardManager>,
    ) -> Self {
        Self {
            database,
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
        let metrics = self.database.read().await.get_performance_metrics();
        
        let grpc_metrics = PerformanceMetrics {
            avg_query_time_ms: metrics.avg_query_time_ms,
            p95_query_time_ms: metrics.p95_query_time_ms,
            p99_query_time_ms: metrics.p99_query_time_ms,
            total_queries: metrics.total_queries,
            queries_per_second: metrics.queries_per_second,
            cache_hits: metrics.cache_hits,
            cache_misses: metrics.cache_misses,
            memory_usage_mb: metrics.memory_usage_mb,
        };
        
        let response = GetMetricsResponse {
            metrics: Some(grpc_metrics),
            error: None,
        };
        Ok(Response::new(response))
    }

    // TODO: 实现其他gRPC方法
    async fn upsert_vector(&self, _request: Request<UpsertVectorRequest>) -> Result<Response<UpsertVectorResponse>, Status> {
        Err(Status::unimplemented("upsert_vector not implemented yet"))
    }

    async fn delete_vector(&self, _request: Request<DeleteVectorRequest>) -> Result<Response<DeleteVectorResponse>, Status> {
        Err(Status::unimplemented("delete_vector not implemented yet"))
    }

    async fn search_vectors(&self, _request: Request<SearchVectorRequest>) -> Result<Response<SearchVectorResponse>, Status> {
        Err(Status::unimplemented("search_vectors not implemented yet"))
    }

    async fn get_vector(&self, _request: Request<GetVectorRequest>) -> Result<Response<GetVectorResponse>, Status> {
        Err(Status::unimplemented("get_vector not implemented yet"))
    }

    async fn join_cluster(&self, _request: Request<JoinClusterRequest>) -> Result<Response<JoinClusterResponse>, Status> {
        Err(Status::unimplemented("join_cluster not implemented yet"))
    }

    async fn leave_cluster(&self, _request: Request<LeaveClusterRequest>) -> Result<Response<LeaveClusterResponse>, Status> {
        Err(Status::unimplemented("leave_cluster not implemented yet"))
    }

    async fn get_cluster_info(&self, _request: Request<GetClusterInfoRequest>) -> Result<Response<GetClusterInfoResponse>, Status> {
        Err(Status::unimplemented("get_cluster_info not implemented yet"))
    }

    async fn heartbeat(&self, _request: Request<HeartbeatRequest>) -> Result<Response<HeartbeatResponse>, Status> {
        Err(Status::unimplemented("heartbeat not implemented yet"))
    }

    async fn append_entries(&self, _request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesResponse>, Status> {
        Err(Status::unimplemented("append_entries not implemented yet"))
    }

    async fn request_vote(&self, _request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteResponse>, Status> {
        Err(Status::unimplemented("request_vote not implemented yet"))
    }

    async fn install_snapshot(&self, _request: Request<InstallSnapshotRequest>) -> Result<Response<InstallSnapshotResponse>, Status> {
        Err(Status::unimplemented("install_snapshot not implemented yet"))
    }

    async fn migrate_shard(&self, _request: Request<MigrateShardRequest>) -> Result<Response<MigrateShardResponse>, Status> {
        Err(Status::unimplemented("migrate_shard not implemented yet"))
    }

    async fn rebalance_shards(&self, _request: Request<RebalanceShardsRequest>) -> Result<Response<RebalanceShardsResponse>, Status> {
        Err(Status::unimplemented("rebalance_shards not implemented yet"))
    }

    async fn get_shard_info(&self, _request: Request<GetShardInfoRequest>) -> Result<Response<GetShardInfoResponse>, Status> {
        Err(Status::unimplemented("get_shard_info not implemented yet"))
    }
}

/// 启动gRPC服务器
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    database: Arc<RwLock<VectorDatabase>>,
    raft_node: Arc<RaftNode>,
    cluster_manager: Arc<ClusterManager>,
    shard_manager: Arc<ShardManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = VectorDbServiceImpl::new(database, raft_node, cluster_manager, shard_manager);
    
    info!("启动gRPC服务器，监听地址: {}", addr);

    Server::builder()
        .add_service(VectorDbServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
} 