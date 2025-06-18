use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 向量点结构体，用于存储向量数据和元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    /// 点的唯一标识符
    pub id: String,
    /// 向量数据
    pub vector: Vec<f32>,
    /// 元数据
    pub payload: HashMap<String, serde_json::Value>,
}

/// 稀疏向量结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SparseVector {
    /// 非零元素的索引
    pub indices: Vec<u32>,
    /// 对应的值
    pub values: Vec<f32>,
    /// 向量维度
    pub dimension: usize,
}

impl SparseVector {
    /// 创建新的稀疏向量
    pub fn new(
        indices: Vec<u32>,
        values: Vec<f32>,
        dimension: usize,
    ) -> Result<Self, VectorDbError> {
        if indices.len() != values.len() {
            return Err(VectorDbError::ConfigError(
                "稀疏向量的索引和值数量不匹配".to_string(),
            ));
        }

        if indices.iter().any(|&i| i as usize >= dimension) {
            return Err(VectorDbError::ConfigError(
                "稀疏向量索引超出维度范围".to_string(),
            ));
        }

        Ok(Self {
            indices,
            values,
            dimension,
        })
    }

    /// 计算稀疏向量的L2范数
    pub fn norm(&self) -> f32 {
        self.values.iter().map(|&v| v * v).sum::<f32>().sqrt()
    }

    /// 计算与另一个稀疏向量的点积
    pub fn dot_product(&self, other: &SparseVector) -> f32 {
        let mut result = 0.0;
        let mut i = 0;
        let mut j = 0;

        while i < self.indices.len() && j < other.indices.len() {
            match self.indices[i].cmp(&other.indices[j]) {
                std::cmp::Ordering::Equal => {
                    result += self.values[i] * other.values[j];
                    i += 1;
                    j += 1;
                }
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
            }
        }

        result
    }

    /// 计算余弦相似度
    pub fn cosine_similarity(&self, other: &SparseVector) -> f32 {
        let dot = self.dot_product(other);
        let norm_product = self.norm() * other.norm();

        if norm_product == 0.0 {
            0.0
        } else {
            dot / norm_product
        }
    }
}

/// 文档的稀疏表示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSparseRepresentation {
    /// 文档ID
    pub document_id: String,
    /// 稀疏向量
    pub sparse_vector: SparseVector,
    /// 文档长度（用于BM25计算）
    pub document_length: f32,
    /// 词频统计
    pub term_frequencies: HashMap<u32, f32>,
}

/// BM25 相关统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25Stats {
    /// 文档总数
    pub total_documents: usize,
    /// 平均文档长度
    pub average_document_length: f32,
    /// 词汇表大小
    pub vocabulary_size: usize,
    /// 文档频率（每个词在多少个文档中出现）
    pub document_frequencies: HashMap<u32, usize>,
}

/// 搜索请求
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// 查询向量
    pub vector: Vec<f32>,
    /// 返回结果数量
    pub limit: usize,
    /// 过滤条件（可选）
    pub filter: Option<Filter>,
    /// 搜索参数
    pub params: Option<SearchParams>,
}

/// 搜索响应
#[derive(Debug, Clone)]
pub struct SearchResponse {
    /// 搜索结果
    pub results: Vec<SearchResult>,
    /// 查询耗时（毫秒）
    pub query_time_ms: f64,
    /// 总匹配数量
    pub total_matches: usize,
}

/// gRPC 搜索响应
#[derive(Debug, Clone)]
pub struct GrpcSearchResponse {
    /// 搜索结果
    pub results: Vec<InternalSearchResult>,
    /// 查询耗时（毫秒）
    pub query_time_ms: f64,
    /// 总匹配数量
    pub total_matches: usize,
}

/// 搜索参数
#[derive(Debug, Clone)]
pub struct SearchParams {
    /// 搜索精度参数
    pub ef: Option<usize>,
    /// 是否返回向量数据
    pub with_vector: bool,
    /// 是否返回元数据
    pub with_payload: bool,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            ef: None,
            with_vector: false,
            with_payload: true,
        }
    }
}

/// 过滤条件
#[derive(Debug, Clone)]
pub enum Filter {
    /// 必须匹配的条件
    Must(Vec<Condition>),
    /// 应该匹配的条件（至少一个）
    Should(Vec<Condition>),
    /// 不能匹配的条件
    MustNot(Vec<Condition>),
    /// 嵌套过滤器
    Nested { path: String, filter: Box<Filter> },
}

/// 过滤条件
#[derive(Debug, Clone)]
pub enum Condition {
    /// 字段等于某个值
    Equals {
        field: String,
        value: serde_json::Value,
    },
    /// 字段在某个范围内
    Range {
        field: String,
        gte: Option<serde_json::Value>,
        lte: Option<serde_json::Value>,
    },
    /// 字段匹配某个模式
    Match { field: String, text: String },
}

/// 混合搜索请求
#[derive(Debug, Clone)]
pub struct HybridSearchRequest {
    /// 密集向量查询（可选）
    pub dense_vector: Option<Vec<f32>>,
    /// 稀疏向量查询（可选）
    pub sparse_vector: Option<SparseVector>,
    /// 文本查询（可选）
    pub text_query: Option<String>,
    /// 返回结果数量
    pub limit: usize,
    /// 密集向量权重
    pub dense_weight: f32,
    /// 稀疏向量权重
    pub sparse_weight: f32,
    /// 文本搜索权重
    pub text_weight: f32,
}

/// 融合策略枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FusionStrategy {
    /// 倒数排名融合 (Reciprocal Rank Fusion)
    RRF { k: f32 },
    /// 线性权重融合
    Linear {
        dense_weight: f32,
        sparse_weight: f32,
        text_weight: f32,
    },
    /// 标准化分数融合
    Normalized {
        dense_weight: f32,
        sparse_weight: f32,
        text_weight: f32,
    },
    /// 学习式融合 - 基于统计的动态权重
    Learned {
        /// 基础权重
        base_weights: FusionWeights,
        /// 根据查询类型调整权重
        query_type_adaptation: bool,
        /// 根据结果质量调整权重
        quality_adaptation: bool,
    },
    /// 自适应融合 - 根据用户反馈学习
    Adaptive {
        /// 初始权重
        initial_weights: FusionWeights,
        /// 学习率
        learning_rate: f32,
        /// 历史窗口大小
        history_size: usize,
    },
}

/// 融合权重配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionWeights {
    pub dense_weight: f32,
    pub sparse_weight: f32,
    pub text_weight: f32,
}

impl Default for FusionWeights {
    fn default() -> Self {
        Self {
            dense_weight: 0.7,
            sparse_weight: 0.2,
            text_weight: 0.1,
        }
    }
}

impl Default for FusionStrategy {
    fn default() -> Self {
        Self::RRF { k: 60.0 }
    }
}

/// 融合策略性能统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionPerformanceStats {
    /// 策略名称
    pub strategy_name: String,
    /// 平均查询时间 (ms)
    pub avg_query_time_ms: f64,
    /// P95 查询时间 (ms)
    pub p95_query_time_ms: f64,
    /// 平均精确度
    pub avg_precision: f64,
    /// 平均召回率
    pub avg_recall: f64,
    /// 用户点击率 (CTR)
    pub click_through_rate: f64,
    /// 总查询数
    pub total_queries: u64,
}

/// 查询性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetrics {
    /// 查询唯一ID
    pub query_id: String,
    /// 查询文本
    pub query_text: String,
    /// 查询时间戳
    pub timestamp: u64,
    /// 查询耗时 (ms)
    pub duration_ms: f64,
    /// 返回结果数
    pub result_count: usize,
    /// 用户点击的结果ID列表
    pub clicked_results: Vec<String>,
    /// 用户满意度评分 (1-5)
    pub user_satisfaction: Option<u8>,
    /// 融合策略
    pub fusion_strategy: String,
}

/// 文档结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// 文档唯一标识符
    pub id: String,
    /// 文档内容
    pub content: String,
    /// 文档标题（可选）
    pub title: Option<String>,
    /// 语言代码（可选）
    pub language: Option<String>,
    /// 包名（可选）
    pub package_name: Option<String>,
    /// 版本号（可选）
    pub version: Option<String>,
    /// 文档类型（可选）
    pub doc_type: Option<String>,
    /// 向量数据（可选）
    pub vector: Option<Vec<f32>>,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 更新时间
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Default for Document {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: String::new(),
            content: String::new(),
            title: None,
            language: None,
            package_name: None,
            version: None,
            doc_type: None,
            vector: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// 文档记录（内部存储格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    /// 文档唯一标识符
    pub id: String,
    /// 文档内容
    pub content: String,
    /// 文档标题
    pub title: String,
    /// 语言代码
    pub language: String,
    /// 包名
    pub package_name: String,
    /// 版本号
    pub version: String,
    /// 文档类型
    pub doc_type: String,
    /// 向量数据（可选）
    pub vector: Option<Vec<f32>>,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 密集嵌入向量
    pub embedding: Vec<f32>,
    /// 稀疏向量表示（可选）
    pub sparse_representation: Option<DocumentSparseRepresentation>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 更新时间
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 文档记录
    pub document: DocumentRecord,
    /// 相似度分数
    pub score: f32,
    /// 相关性分数（可选）
    pub relevance_score: Option<f32>,
    /// 匹配的文本片段（可选）
    pub matched_snippets: Option<Vec<String>>,
}

/// 内部搜索结果（用于gRPC服务）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalSearchResult {
    /// 文档ID
    pub document_id: String,
    /// 文档标题
    pub title: Option<String>,
    /// 内容片段
    pub content_snippet: String,
    /// 相似度分数
    pub similarity_score: f32,
    /// 包名
    pub package_name: Option<String>,
    /// 文档类型
    pub doc_type: Option<String>,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// 分数明细
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    /// 密集向量相似度分数
    pub dense_score: Option<f32>,
    /// 稀疏向量相似度分数
    pub sparse_score: Option<f32>,
    /// 文本匹配分数
    pub text_score: Option<f32>,
    /// 最终融合分数
    pub final_score: f32,
}

/// 向量点（用于HNSW索引）
#[derive(Debug, Clone)]
pub struct VectorPoint {
    /// 向量数据
    pub vector: Vec<f32>,
    /// 关联的文档ID
    pub document_id: String,
}

/// 数据库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// 文档总数
    pub document_count: usize,
    /// 密集向量总数
    pub dense_vector_count: usize,
    /// 稀疏向量总数
    pub sparse_vector_count: usize,
    /// 内存使用量（MB）
    pub memory_usage_mb: f64,
    /// 密集向量索引大小（MB）
    pub dense_index_size_mb: f64,
    /// 稀疏向量索引大小（MB）
    pub sparse_index_size_mb: f64,
    /// 缓存命中率
    pub cache_hit_rate: f64,
    /// BM25 统计信息
    pub bm25_stats: Option<BM25Stats>,
}

/// 性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 平均查询时间（毫秒）
    pub avg_query_time_ms: f64,
    /// P95 查询时间（毫秒）
    pub p95_query_time_ms: f64,
    /// P99 查询时间（毫秒）
    pub p99_query_time_ms: f64,
    /// 总查询数
    pub total_queries: u64,
    /// 每秒查询数
    pub queries_per_second: f64,
    /// 缓存命中次数
    pub cache_hits: u64,
    /// 缓存未命中次数
    pub cache_misses: u64,
    /// 内存使用量（MB）
    pub memory_usage_mb: f64,
}

impl Default for DatabaseStats {
    fn default() -> Self {
        Self {
            document_count: 0,
            dense_vector_count: 0,
            sparse_vector_count: 0,
            memory_usage_mb: 0.0,
            dense_index_size_mb: 0.0,
            sparse_index_size_mb: 0.0,
            cache_hit_rate: 0.0,
            bm25_stats: None,
        }
    }
}

/// 带分数的点，用于搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredPoint {
    /// 点的唯一标识符
    pub id: String,
    /// 向量数据
    pub vector: Vec<f32>,
    /// 元数据
    pub payload: HashMap<String, serde_json::Value>,
    /// 相似度分数
    pub score: f32,
}

impl From<Point> for ScoredPoint {
    fn from(point: Point) -> Self {
        Self {
            id: point.id,
            vector: point.vector,
            payload: point.payload,
            score: 0.0,
        }
    }
}

/// 分布式系统中的节点ID
pub type NodeId = String;

/// Raft算法中的任期号
pub type Term = u64;

/// Raft日志索引
pub type LogIndex = u64;

/// 分片ID
pub type ShardId = u32;

/// 集群配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// 集群 ID
    pub cluster_id: String,
    /// 集群令牌
    pub cluster_token: String,
    /// 节点列表
    pub nodes: Vec<NodeInfo>,
    /// 分片数量
    pub shard_count: u32,
    /// 副本数量
    pub replica_count: u32,
    /// 一致性级别
    pub consistency_level: ConsistencyLevel,
    /// 心跳间隔 (秒)
    pub heartbeat_interval_secs: u64,
    /// 节点超时时间 (秒)
    pub node_timeout_secs: u64,
    /// 最大节点数量
    pub max_nodes: usize,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            cluster_id: "grape-cluster-default".to_string(),
            cluster_token: "default-token".to_string(),
            nodes: Vec::new(),
            shard_count: 16,
            replica_count: 3,
            consistency_level: ConsistencyLevel::Strong,
            heartbeat_interval_secs: 5,
            node_timeout_secs: 30,
            max_nodes: 100,
        }
    }
}

/// 节点负载信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeLoad {
    /// CPU 使用率 (0.0-1.0)
    pub cpu_usage: f64,
    /// 内存使用率 (0.0-1.0)
    pub memory_usage: f64,
    /// 磁盘使用率 (0.0-1.0)
    pub disk_usage: f64,
    /// 请求数量
    pub request_count: u64,
    /// 平均延迟 (毫秒)
    pub avg_latency_ms: f64,
    /// 向量数量
    pub vector_count: u64,
}

impl Default for NodeLoad {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_usage: 0.0,
            request_count: 0,
            avg_latency_ms: 0.0,
            vector_count: 0,
        }
    }
}

/// 节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// 节点ID
    pub id: NodeId,
    /// 节点地址
    pub address: String,
    /// 节点端口
    pub port: u16,
    /// 节点状态
    pub state: NodeState,
    /// 最后心跳时间
    pub last_heartbeat: u64,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 节点负载
    pub load: NodeLoad,
}

/// 节点状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeState {
    /// 健康状态
    Healthy,
    /// 不健康状态
    Unhealthy,
    /// 离线状态
    Offline,
    /// 加入中
    Joining,
    /// 离开中
    Leaving,
}

/// 一致性级别
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsistencyLevel {
    /// 强一致性
    Strong,
    /// 最终一致性
    Eventual,
    /// 会话一致性
    Session,
}

/// 分片信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardInfo {
    /// 分片ID
    pub id: ShardId,
    /// 主节点ID
    pub primary_node: NodeId,
    /// 副本节点ID列表
    pub replica_nodes: Vec<NodeId>,
    /// 分片状态
    pub state: ShardState,
    /// 数据范围
    pub range: ShardRange,
}

/// 分片状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ShardState {
    /// 活跃状态
    Active,
    /// 迁移中
    Migrating,
    /// 分裂中
    Splitting,
    /// 合并中
    Merging,
    /// 不可用
    Unavailable,
}

/// 分片范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRange {
    /// 起始哈希值
    pub start_hash: u64,
    /// 结束哈希值
    pub end_hash: u64,
}

/// 分布式操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedResult<T> {
    /// 操作结果
    pub result: Result<T, String>,
    /// 执行节点ID
    pub node_id: NodeId,
    /// 执行时间戳
    pub timestamp: u64,
}

/// 集群健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    /// 集群状态
    pub status: ClusterStatus,
    /// 健康节点数量
    pub healthy_nodes: usize,
    /// 总节点数量
    pub total_nodes: usize,
    /// 活跃分片数量
    pub active_shards: usize,
    /// 总分片数量
    pub total_shards: usize,
    /// 数据分布均衡度 (0.0-1.0)
    pub balance_score: f32,
}

/// 集群状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClusterStatus {
    /// 绿色：所有功能正常
    Green,
    /// 黄色：部分功能受限
    Yellow,
    /// 红色：严重问题
    Red,
}

/// 集群统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStats {
    /// 总节点数
    pub total_nodes: u32,
    /// 活跃节点数
    pub active_nodes: u32,
    /// 总分片数
    pub total_shards: u32,
    /// 总向量数
    pub total_vectors: u64,
    /// 总存储大小 (GB)
    pub total_storage_gb: f64,
    /// 平均负载
    pub avg_load: NodeLoad,
}

impl Default for ClusterStats {
    fn default() -> Self {
        Self {
            total_nodes: 0,
            active_nodes: 0,
            total_shards: 0,
            total_vectors: 0,
            total_storage_gb: 0.0,
            avg_load: NodeLoad::default(),
        }
    }
}

/// 集群信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    /// 集群配置
    pub config: ClusterConfig,
    /// 节点列表
    pub nodes: HashMap<NodeId, NodeInfo>,
    /// 领导者 ID
    pub leader_id: Option<NodeId>,
    /// 分片映射
    pub shard_map: ShardMap,
    /// 集群统计
    pub stats: ClusterStats,
    /// 集群版本
    pub version: u64,
}

impl Default for ClusterInfo {
    fn default() -> Self {
        Self {
            config: ClusterConfig::default(),
            nodes: HashMap::new(),
            leader_id: None,
            shard_map: ShardMap::default(),
            stats: ClusterStats::default(),
            version: 0,
        }
    }
}

/// 分片映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardMap {
    /// 分片信息
    pub shards: HashMap<u32, ShardInfo>,
    /// 映射版本
    pub version: u64,
}

impl Default for ShardMap {
    fn default() -> Self {
        Self {
            shards: HashMap::new(),
            version: 0,
        }
    }
}

/// 心跳消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    /// 节点 ID
    pub node_id: NodeId,
    /// 节点负载
    pub load: NodeLoad,
    /// 时间戳
    pub timestamp: i64,
}

/// 查询请求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryRequest {
    /// 向量搜索请求
    VectorSearch {
        query_vector: Vec<f32>,
        limit: usize,
        threshold: Option<f32>,
    },
    /// 文本搜索请求
    TextSearch {
        query: String,
        limit: usize,
        filters: Option<HashMap<String, String>>,
    },
    /// 混合搜索请求
    HybridSearch {
        query: String,
        query_vector: Option<Vec<f32>>,
        limit: usize,
        alpha: Option<f32>,
    },
}

/// 查询响应类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryResponse {
    /// 搜索结果
    SearchResults(Vec<SearchResult>),
    /// 错误响应
    Error(String),
    /// 统计信息
    Stats(VectorDbStats),
}

/// 向量数据库错误类型
#[derive(Debug, thiserror::Error)]
pub enum VectorDbError {
    #[error("存储错误: {0}")]
    StorageError(String),
    
    #[error("存储错误: {0}")]
    Storage(String), // Alternative name for compatibility

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("网络错误: {0}")]
    NetworkError(String),

    #[error("配置错误: {0}")]
    ConfigError(String),

    #[error("索引错误: {0}")]
    IndexError(String),

    #[error("查询错误: {0}")]
    QueryError(String),

    #[error("文档未找到: {0}")]
    DocumentNotFound(String),

    #[error("无效的向量维度")]
    InvalidVectorDimension,

    #[error("索引未构建")]
    IndexNotBuilt,

    #[error("维度不匹配: 期望 {expected}, 实际 {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("索引构建错误: {0}")]
    IndexBuildError(String),

    #[error("运行时错误: {0}")]
    RuntimeError(String),

    #[error("认证错误: {0}")]
    AuthError(String),

    #[error("功能未实现: {0}")]
    NotImplemented(String),

    #[error("量化错误: {0}")]
    QuantizationError(String),

    #[error("过滤错误: {0}")]
    FilterError(String),

    #[error("嵌入错误: {0}")]
    EmbeddingError(String),

    #[error("其他错误: {0}")]
    Other(String),
}

impl VectorDbError {
    /// 创建一个通用错误
    pub fn other(msg: impl Into<String>) -> Self {
        VectorDbError::Other(msg.into())
    }

    /// 创建一个嵌入错误
    pub fn embedding_error(msg: impl Into<String>) -> Self {
        VectorDbError::EmbeddingError(msg.into())
    }
}

/// 向量数据库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbStats {
    /// 文档总数
    pub total_documents: usize,
    /// 向量总数
    pub total_vectors: usize,
    /// 索引大小（字节）
    pub index_size_bytes: u64,
    /// 内存使用量（字节）
    pub memory_usage_bytes: u64,
    /// 最后优化时间
    pub last_optimization: Option<chrono::DateTime<chrono::Utc>>,
}

/// 向量数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbConfig {
    /// 数据库路径
    pub db_path: String,
    /// 向量维度
    pub vector_dimension: usize,
    /// 索引类型
    pub index_type: String,
    /// 最大文档数量
    pub max_documents: Option<usize>,
    /// 缓存大小
    pub cache_size: usize,
    /// 是否启用压缩
    pub enable_compression: bool,
    /// 备份间隔（秒）
    pub backup_interval_seconds: Option<u64>,
    /// 嵌入配置
    pub embedding: EmbeddingConfig,
}

/// 嵌入配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// 提供商
    pub provider: String,
    /// API 端点
    pub endpoint: Option<String>,
    /// API 密钥
    pub api_key: Option<String>,
    /// 模型名称
    pub model: String,
    /// API 版本
    pub api_version: Option<String>,
}

impl Default for VectorDbConfig {
    fn default() -> Self {
        Self {
            db_path: "./vector_db".to_string(),
            vector_dimension: 768,
            index_type: "hnsw".to_string(),
            max_documents: None,
            cache_size: 1000,
            enable_compression: false,
            backup_interval_seconds: None,
            embedding: EmbeddingConfig::default(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            endpoint: None,
            api_key: None,
            model: "text-embedding-ada-002".to_string(),
            api_version: None,
        }
    }
}
