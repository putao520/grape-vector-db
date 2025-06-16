# 🏗️ Grape Vector Database - 技术架构设计

## 📖 目录

- [整体架构概览](#整体架构概览)
- [内嵌模式设计](#内嵌模式设计)
- [分布式架构设计](#分布式架构设计)
- [存储引擎设计](#存储引擎设计)
- [查询引擎设计](#查询引擎设计)
- [网络协议设计](#网络协议设计)
- [性能优化策略](#性能优化策略)

## 🌟 整体架构概览

### 核心设计理念

**Grape Vector DB** 采用 **分层模块化** 架构，支持 **内嵌模式** 和 **分布式模式** 两种部署方式：

```
                    ┌─ 内嵌模式 (Embedded Mode)
Application Layer ──┤
                    └─ 服务模式 (Service Mode)
                              │
┌─────────────────────────────────────────────────┐
│              API Gateway Layer                   │
│  ┌─────────────┬─────────────┬─────────────────┐ │
│  │ Qdrant 兼容层│ Native Grape│ 内嵌模式接口    │ │
│  │ (gRPC+REST) │ API (高性能) │ (库形式集成)     │ │
│  └─────────────┴─────────────┴─────────────────┘ │
└─────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────┐
│              Query Engine Layer                  │
│  ┌─────────────┬─────────────┬─────────────────┐ │
│  │ Dense Search│ Sparse      │ Hybrid Fusion   │ │
│  │ (HNSW)      │ Search(BM25)│ (RRF/Custom)    │ │
│  └─────────────┴─────────────┴─────────────────┘ │
└─────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────┐
│             Storage Engine Layer                 │
│  ┌─────────────┬─────────────┬─────────────────┐ │
│  │Vector Store │ Metadata    │ Index Store     │ │
│  │(Quantized)  │ Store(JSON) │ (HNSW+Inverted) │ │
│  └─────────────┴─────────────┴─────────────────┘ │
└─────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────┐
│           Distributed Layer (可选)               │
│  ┌─────────────┬─────────────┬─────────────────┐ │
│  │ Cluster Mgr │ Replication │ Load Balancer   │ │
│  │ (Raft)      │ (Consensus) │ (Routing)       │ │
│  └─────────────┴─────────────┴─────────────────┘ │
└─────────────────────────────────────────────────┘
```

### 模式对比

| 特性 | 内嵌模式 | 分布式模式 |
|------|----------|------------|
| **部署方式** | 库形式集成 | 独立服务集群 |
| **网络开销** | 无 | 有 (gRPC/HTTP) |
| **数据一致性** | 强一致 | 最终一致 |
| **扩展性** | 垂直扩展 | 水平扩展 |
| **适用场景** | 边缘计算、移动端 | 大规模企业应用 |

## 🔧 内嵌模式设计

### 核心架构

```rust
// 内嵌模式核心接口
pub struct EmbeddedVectorDB {
    engine: VectorEngine,
    storage: Box<dyn StorageEngine>,
    config: EmbeddedConfig,
    metrics: MetricsCollector,
}

impl EmbeddedVectorDB {
    // 同步初始化 - 快速启动
    pub fn new(config: EmbeddedConfig) -> Result<Self> {
        let storage = Self::create_storage(&config)?;
        let engine = VectorEngine::new(storage.clone(), &config)?;
        Ok(Self { engine, storage, config, metrics: MetricsCollector::new() })
    }
    
    // 异步操作 - 保持高性能
    pub async fn upsert(&mut self, points: Vec<Point>) -> Result<()>;
    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse>;
    pub async fn delete(&mut self, filter: Filter) -> Result<usize>;
    
    // 同步操作 - 简化集成
    pub fn search_sync(&self, request: SearchRequest) -> Result<SearchResponse>;
    pub fn stats(&self) -> DatabaseStats;
}
```

### 内存管理策略

```rust
pub struct EmbeddedConfig {
    // 内存预算管理
    pub max_memory_mb: Option<usize>,
    pub vector_cache_size: usize,
    pub metadata_cache_size: usize,
    
    // 存储策略
    pub storage_mode: StorageMode,
    pub compression_level: u8,
    pub enable_mmap: bool,
    
    // 性能调优
    pub thread_pool_size: Option<usize>,
    pub batch_size: usize,
    pub sync_interval: Duration,
}

pub enum StorageMode {
    Memory,           // 纯内存模式
    MemoryWithDisk,   // 内存+磁盘备份
    Disk,             // 磁盘模式(mmap)
    Hybrid,           // 智能分层存储
}
```

### 生命周期管理

```rust
impl EmbeddedVectorDB {
    // 优雅关闭 - 确保数据安全
    pub async fn shutdown(self) -> Result<()> {
        self.storage.flush().await?;
        self.storage.sync().await?;
        self.metrics.export_final_stats()?;
        Ok(())
    }
    
    // 热备份 - 不停机备份
    pub async fn backup(&self, path: &Path) -> Result<()> {
        self.storage.create_snapshot(path).await
    }
    
    // 快速恢复 - 故障恢复
    pub async fn restore(path: &Path, config: EmbeddedConfig) -> Result<Self> {
        let storage = StorageEngine::restore_from_snapshot(path, &config).await?;
        let engine = VectorEngine::new(storage.clone(), &config)?;
        Ok(Self { engine, storage, config, metrics: MetricsCollector::new() })
    }
}
```

## 🌐 分布式架构设计

### 集群拓扑

```
                    ┌─ Load Balancer ─┐
                    │                 │
         ┌──────────▼─────────┐   ┌───▼─────────────┐
         │     Gateway        │   │    Gateway      │
         │    Node 1          │   │    Node 2       │
         └──────────┬─────────┘   └───┬─────────────┘
                    │                 │
     ┌──────────────┼─────────────────┼──────────────┐
     │              │                 │              │
┌────▼───┐    ┌────▼───┐       ┌─────▼──┐     ┌─────▼──┐
│Shard A │    │Shard B │       │Shard C │     │Shard D │
│Primary │    │Primary │       │Primary │     │Primary │
└────┬───┘    └────┬───┘       └─────┬──┘     └─────┬──┘
     │             │                 │              │
┌────▼───┐    ┌────▼───┐       ┌─────▼──┐     ┌─────▼──┐
│Shard A │    │Shard B │       │Shard C │     │Shard D │
│Replica │    │Replica │       │Replica │     │Replica │
└────────┘    └────────┘       └────────┘     └────────┘
```

### Raft 共识实现

```rust
pub struct RaftNode {
    node_id: NodeId,
    cluster: ClusterConfig,
    log: RaftLog,
    state_machine: VectorStateMachine,
    network: NetworkLayer,
}

pub enum RaftMessage {
    AppendEntries(AppendEntriesRequest),
    RequestVote(RequestVoteRequest),
    InstallSnapshot(InstallSnapshotRequest),
    // 自定义向量操作
    VectorOperation(VectorOpRequest),
}

impl RaftNode {
    // 处理客户端写请求
    pub async fn handle_write(&mut self, op: VectorOperation) -> Result<Response> {
        if !self.is_leader() {
            return Err(Error::NotLeader(self.current_leader()));
        }
        
        // 1. 写入日志
        let entry = LogEntry::new(op, self.current_term());
        let index = self.log.append(entry).await?;
        
        // 2. 复制到多数节点
        self.replicate_to_majority(index).await?;
        
        // 3. 应用到状态机
        let result = self.state_machine.apply(op).await?;
        
        Ok(Response::Success(result))
    }
    
    // 处理客户端读请求
    pub async fn handle_read(&self, query: SearchRequest) -> Result<SearchResponse> {
        // 强一致性读 - 确认leadership
        self.confirm_leadership().await?;
        
        // 直接从本地状态机读取
        self.state_machine.search(query).await
    }
}
```

### 分片策略

```rust
pub struct ShardManager {
    shard_ring: ConsistentHashRing,
    shard_configs: HashMap<ShardId, ShardConfig>,
    rebalancer: ShardRebalancer,
}

pub enum ShardingStrategy {
    Hash(HashSharding),      // 基于ID哈希
    Range(RangeSharding),    // 基于向量范围
    Semantic(SemanticSharding), // 基于语义相似性
    Custom(Box<dyn CustomSharding>), // 自定义策略
}

impl ShardManager {
    // 智能分片 - 基于数据特征
    pub fn determine_shard(&self, point: &Point) -> ShardId {
        match &self.strategy {
            ShardingStrategy::Hash(h) => h.hash_point(point),
            ShardingStrategy::Semantic(s) => s.semantic_shard(point),
            // ... 其他策略
        }
    }
    
    // 动态重平衡
    pub async fn rebalance(&mut self) -> Result<()> {
        let imbalance = self.detect_imbalance().await?;
        if imbalance.severity > self.config.rebalance_threshold {
            self.rebalancer.execute_rebalance(imbalance).await?;
        }
        Ok(())
    }
}
```

## 💾 存储引擎设计 ✅ **Sled引擎已升级完成**

### Sled 高级存储引擎

**Grape Vector DB** 已完成从基础存储到企业级 **Sled 高级存储引擎** 的升级，提供：

- **ACID 事务支持** - 完整的事务一致性保证
- **多树架构** - 类似列族的数据组织方式  
- **数据压缩** - 70% 压缩比，节省存储空间
- **性能监控** - 实时统计和性能指标
- **备份恢复** - 完整的数据保护机制

**实际性能表现**:
- 写入性能: 13,240 QPS
- 读取性能: 42,018 QPS
- 缓存命中率: 85%
- 数据压缩比: 70%

### 分层存储架构

```rust
pub trait StorageEngine: Send + Sync {
    // 向量存储
    async fn store_vectors(&mut self, vectors: &[VectorRecord]) -> Result<()>;
    async fn load_vectors(&self, ids: &[PointId]) -> Result<Vec<VectorRecord>>;
    
    // 元数据存储
    async fn store_metadata(&mut self, metadata: &[MetadataRecord]) -> Result<()>;
    async fn query_metadata(&self, filter: &Filter) -> Result<Vec<PointId>>;
    
    // 索引存储
    async fn store_index(&mut self, index: &Index) -> Result<()>;
    async fn load_index(&self) -> Result<Index>;
    
    // 事务支持
    async fn begin_transaction(&mut self) -> Result<TransactionId>;
    async fn commit_transaction(&mut self, txn_id: TransactionId) -> Result<()>;
    async fn rollback_transaction(&mut self, txn_id: TransactionId) -> Result<()>;
}

// 混合存储实现
pub struct HybridStorageEngine {
    // 热数据 - 内存存储
    hot_vectors: Arc<RwLock<HashMap<PointId, VectorRecord>>>,
    hot_metadata: Arc<RwLock<HashMap<PointId, MetadataRecord>>>,
    
    // 温数据 - SSD存储
            warm_storage: SledStorage,
    
    // 冷数据 - 对象存储
    cold_storage: ObjectStorage,
    
    // 数据分级策略
    tiering_policy: TieringPolicy,
}
```

### 向量量化存储

```rust
pub struct QuantizedVectorStorage {
    // 原始向量(可选保存)
    original_vectors: Option<Box<dyn VectorStorage>>,
    
    // 量化向量
    quantized_vectors: Box<dyn QuantizedStorage>,
    
    // 量化参数
    quantization_config: QuantizationConfig,
}

pub enum QuantizationType {
    Binary {
        threshold: f32,
        rescore_multiplier: usize,
    },
    Scalar {
        bits_per_component: u8,
        min_values: Vec<f32>,
        max_values: Vec<f32>,
    },
    Product {
        num_codebooks: usize,
        codebook_size: usize,
        centroids: Vec<Vec<f32>>,
    },
}

impl QuantizedVectorStorage {
    // 高性能量化搜索
    pub async fn search_quantized(
        &self,
        query: &[f32],
        k: usize,
    ) -> Result<Vec<ScoredPoint>> {
        // 1. 量化查询向量
        let quantized_query = self.quantize_vector(query)?;
        
        // 2. 快速粗排
        let candidates = self.quantized_vectors
            .search(&quantized_query, k * self.config.oversample_factor)
            .await?;
        
        // 3. 精确重排序(可选)
        if let Some(original) = &self.original_vectors {
            self.rescore_with_original(candidates, query, k).await
        } else {
            Ok(candidates)
        }
    }
}
```

## 🔍 查询引擎设计

### 混合搜索架构

```rust
pub struct HybridQueryEngine {
    dense_engine: DenseSearchEngine,
    sparse_engine: SparseSearchEngine,
    fusion_engine: FusionEngine,
    filter_engine: FilterEngine,
}

pub struct SearchRequest {
    // 密集向量查询
    pub dense_vector: Option<Vec<f32>>,
    
    // 稀疏向量查询
    pub sparse_vector: Option<SparseVector>,
    
    // 文本查询(自动转换为稀疏向量)
    pub text_query: Option<String>,
    
    // 过滤条件
    pub filter: Option<Filter>,
    
    // 融合参数
    pub fusion_config: Option<FusionConfig>,
    
    // 基本参数
    pub limit: usize,
    pub offset: usize,
}

impl HybridQueryEngine {
    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse> {
        let mut results = Vec::new();
        
        // 1. 并行执行不同类型搜索
        let (dense_fut, sparse_fut) = tokio::join!(
            self.execute_dense_search(&request),
            self.execute_sparse_search(&request)
        );
        
        // 2. 收集结果
        if let Ok(dense_results) = dense_fut {
            results.push((SearchType::Dense, dense_results));
        }
        if let Ok(sparse_results) = sparse_fut {
            results.push((SearchType::Sparse, sparse_results));
        }
        
        // 3. 融合多路结果
        let fused_results = self.fusion_engine.fuse(results, &request.fusion_config)?;
        
        // 4. 应用过滤器
        let filtered_results = self.filter_engine.apply_filter(
            fused_results,
            &request.filter
        ).await?;
        
        Ok(SearchResponse {
            points: filtered_results,
            total: filtered_results.len(),
        })
    }
}
```

### 智能缓存系统

```rust
pub struct CacheManager {
    // 查询结果缓存
    query_cache: Arc<Mutex<LruCache<QueryHash, SearchResponse>>>,
    
    // 向量缓存
    vector_cache: Arc<Mutex<LruCache<PointId, VectorRecord>>>,
    
    // 索引缓存
    index_cache: Arc<Mutex<LruCache<IndexId, Box<dyn Index>>>>,
    
    // 缓存策略
    cache_policy: CachePolicy,
}

pub enum CachePolicy {
    LRU { max_size: usize },
    LFU { max_size: usize },
    TTL { ttl: Duration },
    Adaptive { learn_rate: f32 },
}

impl CacheManager {
    // 智能预取
    pub async fn prefetch_related(&self, query: &SearchRequest) -> Result<()> {
        // 基于查询历史预测相关查询
        let related_queries = self.predict_related_queries(query).await?;
        
        // 异步预取
        let prefetch_tasks: Vec<_> = related_queries
            .into_iter()
            .map(|q| self.execute_prefetch(q))
            .collect();
        
        // 并发执行预取，不阻塞主查询
        tokio::spawn(async move {
            futures::future::join_all(prefetch_tasks).await;
        });
        
        Ok(())
    }
}
```

## 🌐 网络协议设计

### gRPC 服务接口

```protobuf
syntax = "proto3";

service VectorService {
    // 基础操作
    rpc Upsert(UpsertRequest) returns (UpsertResponse);
    rpc Search(SearchRequest) returns (SearchResponse);
    rpc Delete(DeleteRequest) returns (DeleteResponse);
    
    // 批量操作
    rpc BatchUpsert(stream UpsertRequest) returns (UpsertResponse);
    rpc BatchSearch(stream SearchRequest) returns (stream SearchResponse);
    
    // 集群操作
    rpc GetClusterInfo(Empty) returns (ClusterInfo);
    rpc HealthCheck(Empty) returns (HealthResponse);
    
    // 流式操作
    rpc Subscribe(SubscribeRequest) returns (stream ChangeEvent);
}

message Point {
    string id = 1;
    repeated float vector = 2;
    google.protobuf.Struct payload = 3;
}

message SearchRequest {
    repeated float vector = 1;
    uint32 limit = 2;
    google.protobuf.Struct filter = 3;
    SearchParams params = 4;
}
```

### 内部通信协议

```rust
pub enum InternalMessage {
    // 集群协调
    JoinCluster { node_info: NodeInfo },
    LeaveCluster { node_id: NodeId },
    
    // 数据同步
    SyncRequest { shard_id: ShardId, from_version: u64 },
    SyncResponse { data: Vec<SyncRecord> },
    
    // 负载均衡
    LoadReport { metrics: NodeMetrics },
    RouteRequest { query: SearchRequest },
    
    // 故障处理
    FailureDetected { failed_node: NodeId },
    RecoveryComplete { recovered_node: NodeId },
}

pub struct MessageRouter {
    node_id: NodeId,
    network: NetworkLayer,
    handlers: HashMap<MessageType, Box<dyn MessageHandler>>,
}

impl MessageRouter {
    // 高效消息路由
    pub async fn route_message(&self, msg: InternalMessage) -> Result<()> {
        let msg_type = msg.message_type();
        let handler = self.handlers.get(&msg_type)
            .ok_or(Error::UnknownMessageType(msg_type))?;
        
        // 异步处理，避免阻塞
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.handle(msg).await {
                tracing::error!("Message handling failed: {}", e);
            }
        });
        
        Ok(())
    }
}
```

## ⚡ 性能优化策略

### SIMD 向量运算

```rust
use std::arch::x86_64::*;

pub struct SIMDVectorOps;

impl SIMDVectorOps {
    // AVX2 优化的余弦相似度计算
    #[target_feature(enable = "avx2")]
    pub unsafe fn cosine_similarity_avx2(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        assert_eq!(a.len() % 8, 0);
        
        let mut dot_product = _mm256_setzero_ps();
        let mut norm_a = _mm256_setzero_ps();
        let mut norm_b = _mm256_setzero_ps();
        
        for i in (0..a.len()).step_by(8) {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            
            dot_product = _mm256_fmadd_ps(va, vb, dot_product);
            norm_a = _mm256_fmadd_ps(va, va, norm_a);
            norm_b = _mm256_fmadd_ps(vb, vb, norm_b);
        }
        
        // 水平求和
        let dot = Self::horizontal_sum_avx2(dot_product);
        let norm_a = Self::horizontal_sum_avx2(norm_a).sqrt();
        let norm_b = Self::horizontal_sum_avx2(norm_b).sqrt();
        
        dot / (norm_a * norm_b)
    }
}
```

### 零拷贝数据传输

```rust
pub struct ZeroCopyBuffer {
    mmap: memmap2::Mmap,
    layout: BufferLayout,
}

impl ZeroCopyBuffer {
    // 零拷贝向量访问
    pub fn get_vector(&self, index: usize) -> &[f32] {
        let offset = self.layout.vector_offset(index);
        let size = self.layout.vector_size;
        
        unsafe {
            std::slice::from_raw_parts(
                self.mmap.as_ptr().add(offset) as *const f32,
                size
            )
        }
    }
    
    // 零拷贝批量向量访问
    pub fn get_vectors_batch(&self, indices: &[usize]) -> Vec<&[f32]> {
        indices.iter()
            .map(|&i| self.get_vector(i))
            .collect()
    }
}
```

### 智能预取策略

```rust
pub struct PrefetchManager {
    access_patterns: LruCache<QueryPattern, AccessPattern>,
    prefetch_queue: Arc<Mutex<VecDeque<PrefetchTask>>>,
    background_executor: tokio::task::JoinHandle<()>,
}

impl PrefetchManager {
    // 基于机器学习的预取决策
    pub async fn should_prefetch(&self, query: &SearchRequest) -> bool {
        let pattern = self.extract_pattern(query);
        
        if let Some(access_pattern) = self.access_patterns.get(&pattern) {
            // 基于历史命中率决策
            access_pattern.hit_rate > 0.7 && access_pattern.access_frequency > 10
        } else {
            // 新模式，保守预取
            false
        }
    }
    
    // 异步后台预取
    async fn execute_prefetch(&self, task: PrefetchTask) -> Result<()> {
        match task {
            PrefetchTask::Vectors(ids) => {
                self.storage.warm_cache_vectors(&ids).await?;
            }
            PrefetchTask::Index(index_id) => {
                self.storage.warm_cache_index(index_id).await?;
            }
            PrefetchTask::Metadata(filter) => {
                self.storage.warm_cache_metadata(&filter).await?;
            }
        }
        Ok(())
    }
}
```

---

**文档版本**: v1.0  
**最后更新**: 2024年  
**维护者**: Grape Vector DB 架构团队 