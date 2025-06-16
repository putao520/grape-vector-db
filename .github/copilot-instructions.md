# Grape Vector Database - GitHub Copilot 指令

## 项目概述

Grape Vector Database 是一个高性能的向量数据库系统，旨在成为 Qdrant 的完整替代方案，提供独特的内嵌模式支持。项目使用 Rust 开发，采用现代化的异步架构。

### 当前状态
- **Phase 1 已完成**: 核心功能（Sled存储、内嵌模式、二进制量化、混合搜索、高级过滤）
- **Phase 2 计划中**: 分布式架构（基于Raft共识算法）

## 核心技术栈

### 存储与索引
- **存储引擎**: Sled (原RocksDB已迁移)
- **向量索引**: instant-distance (HNSW算法)
- **缓存**: moka (多层缓存策略)
- **并发**: parking_lot, crossbeam, dashmap

### 网络与协议
- **gRPC**: tonic + prost
- **异步运行时**: tokio
- **HTTP客户端**: reqwest

### 数据处理
- **序列化**: serde + postcard
- **数学计算**: nalgebra, ndarray
- **并行计算**: rayon
- **压缩**: flate2

## 开发规范 (CRITICAL)

### 代码质量要求
1. **SOLID原则**: 必须严格遵循
2. **单一职责**: 每个函数只干一件事
3. **异常处理**: 所有异常必须处理，使用 thiserror + anyhow
4. **变量命名**: 必须有意义，禁用data、info等泛化命名
5. **完整实现**: 严禁任何TODO、占位符或简化实现，所有功能必须完整

### 错误处理模式
```rust
use thiserror::Error;
use anyhow::Result;

#[derive(Error, Debug)]
pub enum VectorDbError {
    #[error("向量维度不匹配: 期望 {expected}, 实际 {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    
    #[error("存储错误: {0}")]
    Storage(#[from] sled::Error),
    
    #[error("网络错误: {0}")]
    Network(#[from] reqwest::Error),
}
```

### 异步编程规范
- 所有I/O操作必须使用async/await
- 使用tokio作为异步运行时
- 合理使用Arc<Mutex<T>>和Arc<RwLock<T>>
- 优先使用parking_lot提供的锁

### 内存安全
- 避免不必要的clone()操作
- 使用Cow<str>优化字符串处理
- 合理使用smallvec减少小数组分配
- 使用memmap2进行大文件处理

## 架构原则

### 存储层设计
```rust
// 存储层抽象
pub trait StorageBackend: Send + Sync {
    async fn store_vector(&self, id: &str, vector: &[f32]) -> Result<()>;
    async fn retrieve_vector(&self, id: &str) -> Result<Option<Vec<f32>>>;
    async fn delete_vector(&self, id: &str) -> Result<bool>;
}

// Sled实现
pub struct SledStorage {
    db: Arc<sled::Db>,
    trees: Arc<RwLock<HashMap<String, sled::Tree>>>,
}
```

### 向量索引层
```rust
// 索引抽象
pub trait VectorIndex: Send + Sync {
    async fn add_vector(&mut self, id: String, vector: Vec<f32>) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>>;
    async fn remove_vector(&mut self, id: &str) -> Result<bool>;
}

// HNSW实现
pub struct HnswIndex {
    index: Arc<RwLock<instant_distance::Hnsw>>,
    id_map: Arc<RwLock<BiHashMap<String, usize>>>,
}
```

### 嵌入提供商抽象
```rust
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}
```

## 分布式架构计划 (Phase 2)

### Raft共识
- 使用自定义Raft实现
- 基于gRPC的节点通信
- 支持动态成员变更
- 数据分片和复制

### 集群管理
```rust
pub struct ClusterNode {
    id: String,
    address: String,
    role: NodeRole, // Leader, Follower, Candidate
    last_heartbeat: Instant,
}

pub enum NodeRole {
    Leader,
    Follower, 
    Candidate,
}
```

## 性能优化策略

### 内存优化
- 二进制量化 (40x性能提升)
- 智能缓存策略 (85%命中率)
- 内存池复用
- 零拷贝优化

### 并发优化
- 无锁数据结构 (crossbeam, dashmap)
- 读写分离
- 批量操作
- 异步I/O流水线

### 存储优化
- 数据压缩 (70%压缩比)
- 索引预加载
- 写入批处理
- 增量备份

## 测试要求

### 单元测试
- 每个模块必须有完整的单元测试
- 使用proptest进行属性测试
- 错误路径必须测试

### 性能测试
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_vector_search(c: &mut Criterion) {
    c.bench_function("search_1000_vectors", |b| {
        b.iter(|| {
            // 性能测试代码
        })
    });
}
```

### 集成测试
- 端到端测试场景
- 并发安全测试
- 故障恢复测试

## 代码风格

### 命名约定
- 模块名: snake_case
- 结构体: PascalCase  
- 函数名: snake_case
- 常量: SCREAMING_SNAKE_CASE
- 类型别名: PascalCase

### 文档要求
```rust
/// 向量搜索结果
/// 
/// # 示例
/// ```rust
/// let result = SearchResult {
///     id: "doc1".to_string(),
///     score: 0.95,
///     document: Some(doc),
/// };
/// ```
pub struct SearchResult {
    /// 文档唯一标识符
    pub id: String,
    /// 相似度分数 (0.0-1.0)
    pub score: f32,
    /// 关联的文档对象
    pub document: Option<Document>,
}
```

## 日志规范

使用tracing进行结构化日志：

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(self, vectors))]
pub async fn batch_insert(&self, vectors: Vec<(String, Vec<f32>)>) -> Result<()> {
    info!(count = vectors.len(), "开始批量插入向量");
    
    // 实现代码
    
    info!(count = vectors.len(), elapsed = ?start.elapsed(), "批量插入完成");
    Ok(())
}
```

## 部署和构建

### 构建配置
```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
```

### 特性标志
```toml
[features]
default = ["sled-backend", "hnsw-index"]
sled-backend = ["sled"]
rocksdb-backend = ["rocksdb"] # 已移除
hnsw-index = ["instant-distance"]
distributed = ["raft", "cluster"]
```

## 兼容性目标

- **Qdrant API兼容**: 提供类似的REST API
- **向量格式兼容**: 支持主流向量格式
- **嵌入提供商**: 支持OpenAI、Azure、Ollama、HuggingFace等

## 注意事项

1. **响应语言**: 所有代码注释和文档使用中文，错误信息支持中文
2. **依赖管理**: 避免重型依赖，优先使用纯Rust实现
3. **跨平台**: 确保Windows、Linux、macOS兼容
4. **内存使用**: 严格控制内存使用，避免内存泄漏
5. **线程安全**: 所有公共API必须线程安全
