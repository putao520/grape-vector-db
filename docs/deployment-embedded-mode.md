# 🍇 Grape Vector Database - 内嵌模式部署指南

## 📖 概述

内嵌模式允许将 Grape Vector Database 直接集成到你的应用程序中，作为一个库来使用。这种模式特别适合：

- **桌面应用程序**：需要本地向量搜索功能
- **移动应用**：离线向量数据库需求
- **边缘计算**：资源受限环境下的AI应用
- **微服务**：单一服务内的向量数据处理
- **原型开发**：快速开发和测试

## ✨ 核心特性

- **🔄 同步API**：提供阻塞式接口，简化集成
- **⚡ 高性能**：13K+ 写入QPS, 42K+ 读取QPS
- **🛡️ 生命周期管理**：优雅启动/关闭、健康检查
- **🔒 线程安全**：完整的并发访问控制
- **📦 零依赖部署**：单二进制文件，无需外部服务
- **💾 持久化存储**：基于Sled的ACID事务支持

## 🚀 快速开始

### 基本用法

```rust
use grape_vector_db::embedded::EmbeddedVectorDB;
use grape_vector_db::types::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 创建配置
    let config = EmbeddedConfig::default()
        .with_data_dir("./embedded_db")
        .with_vector_dimension(384)
        .with_memory_limit_mb(512)
        .with_warmup(true);

    // 2. 启动数据库
    let mut db = EmbeddedVectorDB::new(config).await?;
    
    // 3. 添加向量
    let point = Point {
        id: "doc_1".to_string(),
        vector: vec![0.1; 384], // 384维向量
        payload: [("title".to_string(), "示例文档".into())].into(),
    };
    
    db.upsert_point(point).await?;
    
    // 4. 搜索向量
    let query = vec![0.1; 384];
    let results = db.search_vectors(&query, 10).await?;
    
    println!("找到 {} 个相似向量", results.len());
    
    // 5. 优雅关闭
    db.close().await?;
    
    Ok(())
}
```

### 同步API模式

对于不使用异步的应用程序，可以使用同步API：

```rust
use grape_vector_db::embedded::EmbeddedVectorDB;
use grape_vector_db::types::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用同步API初始化
    let mut db = EmbeddedVectorDB::new_blocking(
        EmbeddedConfig::default()
            .with_data_dir("./embedded_db")
            .with_vector_dimension(384)
    )?;
    
    // 同步操作
    let point = Point {
        id: "sync_doc".to_string(),
        vector: vec![0.2; 384],
        payload: [("type".to_string(), "同步文档".into())].into(),
    };
    
    db.upsert_point_blocking(point)?;
    
    let query = vec![0.2; 384];
    let results = db.search_vectors_blocking(&query, 5)?;
    
    println!("同步搜索结果: {} 个", results.len());
    
    Ok(())
}
```

## ⚙️ 配置选项

### 基础配置

```rust
use grape_vector_db::embedded::EmbeddedConfig;
use grape_vector_db::advanced_storage::AdvancedStorageConfig;
use grape_vector_db::index::IndexConfig;

let config = EmbeddedConfig {
    // 数据存储目录
    data_dir: "./grape_data".into(),
    
    // 内存限制 (MB)
    max_memory_mb: Some(1024),
    
    // 线程池大小
    thread_pool_size: Some(8),
    
    // 启动超时 (毫秒)
    startup_timeout_ms: 30000,
    
    // 关闭超时 (毫秒)
    shutdown_timeout_ms: 10000,
    
    // 启用预热
    enable_warmup: true,
    
    // 向量维度
    vector_dimension: 1536,
    
    // 存储配置
    storage: AdvancedStorageConfig {
        compression_enabled: true,
        cache_size_mb: 256,
        write_buffer_size_mb: 64,
        max_write_buffer_number: 4,
        target_file_size_mb: 128,
        ..Default::default()
    },
    
    // 索引配置
    index: IndexConfig {
        m: 32,                    // HNSW M 参数
        ef_construction: 400,     // 构建时的ef参数
        ef_search: 200,          // 搜索时的ef参数
        max_m: 64,               // 最大M值
        ml: 1.0 / (2.0_f32.ln()), // 层级概率
        ..Default::default()
    },
};
```

### 性能优化配置

```rust
// 高性能配置
let high_perf_config = EmbeddedConfig::default()
    .with_data_dir("./high_perf_db")
    .with_vector_dimension(768)
    .with_memory_limit_mb(2048)
    .with_thread_pool_size(16)
    .with_warmup(true)
    .with_storage_config(AdvancedStorageConfig {
        compression_enabled: false,  // 关闭压缩以提高性能
        cache_size_mb: 512,         // 更大的缓存
        write_buffer_size_mb: 128,   // 更大的写缓冲
        bloom_filter_bits_per_key: 10,
        ..Default::default()
    })
    .with_index_config(IndexConfig {
        m: 48,
        ef_construction: 800,
        ef_search: 400,
        ..Default::default()
    });

// 内存优化配置  
let memory_optimized_config = EmbeddedConfig::default()
    .with_data_dir("./memory_opt_db")
    .with_vector_dimension(384)
    .with_memory_limit_mb(256)
    .with_thread_pool_size(4)
    .with_storage_config(AdvancedStorageConfig {
        compression_enabled: true,   // 启用压缩节省内存
        cache_size_mb: 64,          // 较小的缓存
        write_buffer_size_mb: 16,    // 较小的写缓冲
        max_write_buffer_number: 2,
        ..Default::default()
    })
    .with_index_config(IndexConfig {
        m: 16,
        ef_construction: 200,
        ef_search: 100,
        ..Default::default()
    });
```

## 🔧 高级功能

### 生命周期管理

```rust
use grape_vector_db::embedded::{EmbeddedVectorDB, DatabaseState};

// 检查数据库状态
async fn monitor_database(db: &EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    // 获取数据库状态
    let state = db.get_state().await;
    println!("数据库状态: {:?}", state);
    
    // 健康检查
    let health = db.health_check().await?;
    println!("健康状态: {:?}", health);
    
    // 获取统计信息
    let stats = db.get_stats().await?;
    println!("文档数量: {}", stats.document_count);
    println!("内存使用: {:.2} MB", stats.memory_usage_mb);
    println!("缓存命中率: {:.2}%", stats.cache_hit_rate * 100.0);
    
    Ok(())
}

// 优雅关闭处理
async fn graceful_shutdown(db: EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    println!("开始优雅关闭...");
    
    // 停止接受新请求
    db.set_read_only(true).await?;
    
    // 等待当前操作完成
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    
    // 执行最终保存
    db.flush().await?;
    
    // 关闭数据库
    db.close().await?;
    
    println!("优雅关闭完成");
    Ok(())
}
```

### 批量操作

```rust
use grape_vector_db::types::Point;

async fn batch_operations(db: &mut EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    // 批量插入
    let points: Vec<Point> = (0..1000)
        .map(|i| Point {
            id: format!("batch_{}", i),
            vector: vec![i as f32 / 1000.0; 384],
            payload: [
                ("batch_id".to_string(), i.into()),
                ("category".to_string(), "batch_data".into()),
            ].into(),
        })
        .collect();
    
    // 执行批量插入
    db.upsert_points_batch(points).await?;
    
    // 批量搜索
    let queries = vec![
        vec![0.1; 384],
        vec![0.5; 384], 
        vec![0.9; 384],
    ];
    
    let batch_results = db.search_vectors_batch(&queries, 10).await?;
    
    for (i, results) in batch_results.iter().enumerate() {
        println!("查询 {} 返回 {} 个结果", i, results.len());
    }
    
    Ok(())
}
```

### 过滤和条件查询

```rust
use grape_vector_db::types::{Filter, Condition};

async fn filtered_search(db: &EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    let query = vec![0.5; 384];
    
    // 创建过滤条件
    let filter = Filter {
        conditions: vec![
            Condition::Equals {
                field: "category".to_string(),
                value: "documents".into(),
            },
            Condition::Range {
                field: "score".to_string(),
                min: Some(0.8.into()),
                max: Some(1.0.into()),
            },
        ],
        operator: crate::types::FilterOperator::And,
    };
    
    // 执行过滤搜索
    let results = db.search_vectors_with_filter(&query, 10, &filter).await?;
    
    println!("过滤搜索返回 {} 个结果", results.len());
    
    Ok(())
}
```

## 📊 监控和指标

### 性能监控

```rust
use grape_vector_db::metrics::MetricsCollector;

async fn collect_metrics(db: &EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    let metrics = db.get_metrics().await?;
    
    println!("性能指标:");
    println!("  查询延迟 P50: {:.2}ms", metrics.query_latency_p50);
    println!("  查询延迟 P95: {:.2}ms", metrics.query_latency_p95);
    println!("  查询延迟 P99: {:.2}ms", metrics.query_latency_p99);
    println!("  QPS: {:.2}", metrics.queries_per_second);
    println!("  索引命中率: {:.2}%", metrics.index_hit_rate * 100.0);
    
    Ok(())
}
```

### 内存使用监控

```rust
async fn monitor_memory(db: &EmbeddedVectorDB) -> Result<(), Box<dyn std::error::Error>> {
    let memory_info = db.get_memory_info().await?;
    
    println!("内存使用情况:");
    println!("  总内存: {:.2} MB", memory_info.total_memory_mb);
    println!("  已用内存: {:.2} MB", memory_info.used_memory_mb);
    println!("  向量内存: {:.2} MB", memory_info.vector_memory_mb);
    println!("  索引内存: {:.2} MB", memory_info.index_memory_mb);
    println!("  缓存内存: {:.2} MB", memory_info.cache_memory_mb);
    
    // 内存清理
    if memory_info.used_memory_mb > 800.0 {
        println!("内存使用过高，执行清理...");
        db.clear_cache().await?;
        db.compact().await?;
    }
    
    Ok(())
}
```

## 🛠️ 故障排除

### 常见问题

1. **启动失败**
   ```
   错误: Database failed to initialize
   解决: 检查数据目录权限和磁盘空间
   ```

2. **内存不足**
   ```
   错误: Out of memory during vector insertion
   解决: 增加max_memory_mb或减少batch_size
   ```

3. **性能问题**
   ```
   错误: Search latency too high
   解决: 调整ef_search参数或增加cache_size_mb
   ```

### 调试模式

```rust
use tracing::{info, debug, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用调试日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    let config = EmbeddedConfig::default()
        .with_data_dir("./debug_db")
        .with_vector_dimension(384);
    
    info!("启动内嵌数据库...");
    let db = EmbeddedVectorDB::new(config).await?;
    
    debug!("数据库启动成功");
    
    // 你的应用逻辑...
    
    Ok(())
}
```

## 📈 性能基准

### 测试环境
- CPU: Intel i7-12700K
- 内存: 32GB DDR4
- 存储: NVMe SSD
- 向量维度: 768

### 基准结果

| 操作类型 | QPS | 延迟 (P95) | 内存使用 |
|---------|-----|-----------|----------|
| 向量插入 | 13,000+ | 5ms | 512MB |
| 向量搜索 | 42,000+ | 2ms | 256MB |
| 批量插入 | 25,000+ | 10ms | 1GB |
| 过滤搜索 | 18,000+ | 8ms | 512MB |

## 🔗 相关链接

- [单节点部署指南](./deployment-single-node.md)
- [3节点集群部署指南](./deployment-cluster-3node.md)
- [API 参考文档](./api-reference.md)
- [配置参考](./configuration-reference.md)
- [性能调优指南](./performance-tuning.md)