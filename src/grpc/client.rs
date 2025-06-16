use std::sync::Arc;
use tonic::transport::Channel;
use tracing::{info, error};

use super::{
    vector_db_service_client::VectorDbServiceClient,
    *,
};

/// gRPC客户端
pub struct VectorDbClient {
    client: VectorDbServiceClient<Channel>,
}

impl VectorDbClient {
    /// 连接到gRPC服务器
    pub async fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("连接到gRPC服务器: {}", endpoint);
        
        let client = VectorDbServiceClient::connect(endpoint.to_string()).await?;
        
        Ok(Self { client })
    }

    /// 添加文档
    pub async fn add_document(&mut self, document: Document) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(AddDocumentRequest {
            document: Some(document),
        });

        let response = self.client.add_document(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        Ok(resp.document_id)
    }

    /// 搜索文档
    pub async fn search_documents(
        &mut self,
        query: String,
        limit: u32,
        filter: Option<String>,
    ) -> Result<SearchDocumentResponse, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(SearchDocumentRequest {
            query,
            limit,
            filter,
        });

        let response = self.client.search_documents(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        Ok(resp)
    }

    /// 获取文档
    pub async fn get_document(&mut self, document_id: String) -> Result<Option<Document>, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(GetDocumentRequest { document_id });

        let response = self.client.get_document(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        Ok(resp.document)
    }

    /// 删除文档
    pub async fn delete_document(&mut self, document_id: String) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(DeleteDocumentRequest { document_id });

        let response = self.client.delete_document(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        Ok(resp.deleted)
    }

    /// 获取统计信息
    pub async fn get_stats(&mut self) -> Result<DatabaseStats, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(GetStatsRequest {});

        let response = self.client.get_stats(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        resp.stats.ok_or_else(|| "No stats returned".into())
    }

    /// 获取性能指标
    pub async fn get_metrics(&mut self) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        let request = tonic::Request::new(GetMetricsRequest {});

        let response = self.client.get_metrics(request).await?;
        let resp = response.into_inner();

        if let Some(error) = resp.error {
            return Err(error.into());
        }

        resp.metrics.ok_or_else(|| "No metrics returned".into())
    }
} 