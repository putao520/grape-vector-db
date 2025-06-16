use rayon::prelude::*;

use crate::{
    types::{SearchResult, VectorDbError},
    storage::VectorStore,
};

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
}

impl ParallelSearchExecutor {
    /// 创建新的并行搜索执行器
    pub fn new(config: ParallelSearchConfig) -> Self {
        Self { config }
    }

    /// 执行并行文本搜索
    pub async fn parallel_text_search(
        &self,
        queries: Vec<String>,
        storage: &dyn VectorStore,
        limit: usize,
    ) -> Result<Vec<Vec<SearchResult>>, VectorDbError> {
        let results: Result<Vec<_>, _> = queries
            .into_par_iter()
            .map(|query| {
                // 注意：这里使用同步版本，因为 rayon 不支持异步
                // 在实际实现中，您可能需要使用不同的并行策略
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        storage.text_search(&query, limit, None).await
                    })
                })
            })
            .collect();

        results
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
                    tokio::runtime::Handle::current().block_on(async {
                        storage.vector_search(&query_vector, limit, None).await
                    })
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