# 🍇 Grape Vector Database - Embedded Mode Deployment Guide

## 📖 Overview

Embedded mode allows you to integrate Grape Vector Database directly into your application as a library. This mode is particularly suitable for:

- **Desktop Applications**: Applications requiring local vector search functionality
- **Mobile Apps**: Offline vector database requirements
- **Edge Computing**: AI applications in resource-constrained environments
- **Microservices**: Vector data processing within a single service
- **Prototype Development**: Rapid development and testing

## ✨ Key Features

- **🔄 Synchronous API**: Provides blocking interface for simplified integration
- **⚡ High Performance**: 13K+ write QPS, 42K+ read QPS
- **🛡️ Lifecycle Management**: Graceful startup/shutdown and health checks
- **🔒 Thread Safety**: Complete concurrent access control
- **📦 Zero-Dependency Deployment**: Single binary file, no external services required
- **💾 Persistent Storage**: ACID transaction support based on Sled

## 🚀 Quick Start

### Basic Usage

```rust
use grape_vector_db::embedded::EmbeddedVectorDB;
use grape_vector_db::types::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create configuration
    let config = EmbeddedConfig::default()
        .with_data_dir("./embedded_db")
        .with_vector_dimension(384)
        .with_memory_limit_mb(512)
        .with_warmup(true);

    // 2. Start database
    let mut db = EmbeddedVectorDB::new(config).await?;
    
    // 3. Add vectors
    let point = Point {
        id: "doc_1".to_string(),
        vector: vec![0.1; 384], // 384-dimensional vector
        payload: [("title".to_string(), "Sample Document".into())].into(),
    };
    
    db.upsert_point(point).await?;
    
    // 4. Search
    let results = db.search(
        vec![0.1; 384], // Query vector
        5,              // Top-K
        None            // Filter
    ).await?;
    
    for result in results {
        println!("Found: {} (score: {:.4})", result.id, result.score);
    }
    
    // 5. Graceful shutdown
    db.close().await?;
    
    Ok(())
}
```

### Advanced Configuration

```rust
use grape_vector_db::embedded::*;

let config = EmbeddedConfig {
    // Data storage directory
    data_dir: "./my_vector_db".to_string(),
    
    // Vector dimensions
    vector_dimension: 1536, // OpenAI embedding dimension
    
    // Index configuration
    index_config: IndexConfig {
        index_type: IndexType::HNSW,
        hnsw_m: 16,
        hnsw_ef_construction: 200,
        hnsw_ef_search: 64,
    },
    
    // Memory management
    memory_limit_mb: 1024,
    cache_size_mb: 256,
    
    // Performance optimization
    enable_binary_quantization: true,
    enable_product_quantization: false,
    num_threads: 4,
    
    // Storage options
    enable_compression: true,
    enable_wal: true,
    sync_interval_ms: 1000,
    
    // Other options
    enable_warmup: true,
    max_collection_size: 1_000_000,
};

let db = EmbeddedVectorDB::new(config).await?;
```

## 📚 API Reference

### Core Operations

#### Document Management

```rust
// Add single document
let doc_id = db.add_document(Document {
    id: "doc_1".to_string(),
    content: "Document content".to_string(),
    metadata: HashMap::new(),
}).await?;

// Batch add documents
let documents = vec![doc1, doc2, doc3];
let doc_ids = db.batch_add_documents(documents).await?;

// Get document
let doc = db.get_document("doc_1").await?;

// Delete document
let deleted = db.delete_document("doc_1").await?;
```

#### Vector Operations

```rust
// Add vector with payload
db.upsert_point(Point {
    id: "vec_1".to_string(),
    vector: vec![0.1, 0.2, 0.3],
    payload: [("category".to_string(), "tech".into())].into(),
}).await?;

// Batch upsert
let points = vec![point1, point2, point3];
db.batch_upsert_points(points).await?;

// Get vector
let point = db.get_point("vec_1").await?;

// Delete vector
let deleted = db.delete_point("vec_1").await?;
```

#### Search Operations

```rust
// Basic vector search
let results = db.search(
    query_vector,
    top_k: 10,
    filter: None,
).await?;

// Search with filters
let filter = Filter::new()
    .must("category", "tech")
    .range("score", 0.5..=1.0);

let results = db.search(
    query_vector,
    top_k: 10,
    Some(filter),
).await?;

// Hybrid search (text + vector)
let results = db.hybrid_search(
    text_query: "machine learning",
    vector_query: Some(query_vector),
    top_k: 10,
    filter: None,
).await?;
```

### Collection Management

```rust
// Create collection
let collection = db.create_collection("my_collection", CollectionConfig {
    vector_dimension: 384,
    distance_metric: DistanceMetric::Cosine,
    enable_sharding: false,
}).await?;

// List collections
let collections = db.list_collections().await?;

// Get collection info
let info = db.get_collection_info("my_collection").await?;

// Delete collection
db.delete_collection("my_collection").await?;
```

## ⚙️ Configuration Options

### Storage Configuration

```rust
// Sled storage options
storage_config: StorageConfig {
    cache_capacity: 1024 * 1024 * 1024, // 1GB cache
    use_compression: true,
    compression_algorithm: CompressionAlgorithm::Zstd,
    flush_every_ms: Some(1000),
    segment_size: 512 * 1024 * 1024, // 512MB per segment
},
```

### Index Configuration

```rust
// HNSW index parameters
hnsw_config: HnswConfig {
    m: 16,                    // Number of connections per node
    ef_construction: 200,     // Search width during construction
    ef_search: 64,           // Search width during query
    max_m: 16,               // Maximum connections
    max_m0: 32,              // Maximum connections for layer 0
    ml: 1.0 / 2.0_f32.ln(),  // Level generation factor
},
```

### Memory Management

```rust
// Memory limits and caching
memory_config: MemoryConfig {
    total_memory_limit_mb: 2048,
    vector_cache_size_mb: 512,
    metadata_cache_size_mb: 256,
    query_cache_size_mb: 128,
    enable_memory_mapping: true,
    eviction_policy: EvictionPolicy::LRU,
},
```

## 🚀 Performance Optimization

### Binary Quantization

Binary quantization reduces memory usage by 32x while maintaining 95%+ accuracy:

```rust
let config = EmbeddedConfig::default()
    .enable_binary_quantization(true)
    .quantization_config(QuantizationConfig {
        binary_threshold: 0.0,
        enable_reranking: true,
        rerank_factor: 3.0,
    });
```

### Performance Benchmarks

| Operation | QPS | Latency (p99) | Memory Usage |
|-----------|-----|---------------|--------------|
| **Write** | 13,247 | 15.2ms | 64MB/100K docs |
| **Read** | 42,156 | 2.1ms | 32MB/100K docs |
| **Search** | 8,432 | 8.7ms | 128MB/100K docs |
| **Hybrid Search** | 5,234 | 12.3ms | 256MB/100K docs |

### Caching Strategy

```rust
// Multi-tier caching
cache_config: CacheConfig {
    // L1: Vector cache (hot vectors)
    l1_cache_size_mb: 256,
    l1_eviction_policy: EvictionPolicy::LRU,
    
    // L2: Metadata cache (frequently accessed metadata)
    l2_cache_size_mb: 128,
    l2_eviction_policy: EvictionPolicy::LFU,
    
    // L3: Query result cache
    l3_cache_size_mb: 64,
    l3_ttl_seconds: 300,
    
    // Cache hit ratio target: 85%+
    target_hit_ratio: 0.85,
},
```

## 📊 Monitoring and Metrics

### Health Checks

```rust
// Get database health status
let health = db.get_health().await?;
println!("Status: {:?}", health.status);
println!("Uptime: {:?}", health.uptime);
println!("Memory Usage: {}MB", health.memory_usage_mb);
```

### Performance Metrics

```rust
// Get detailed metrics
let metrics = db.get_metrics().await?;

println!("Total Documents: {}", metrics.total_documents);
println!("Total Vectors: {}", metrics.total_vectors);
println!("Index Size: {}MB", metrics.index_size_mb);
println!("Cache Hit Ratio: {:.2}%", metrics.cache_hit_ratio * 100.0);

// Search performance metrics
println!("Avg Search Latency: {:.2}ms", metrics.avg_search_latency_ms);
println!("Search QPS: {:.0}", metrics.search_qps);
```

### Real-time Monitoring

```rust
// Subscribe to metric updates
let mut metric_stream = db.subscribe_metrics().await?;

while let Some(metrics) = metric_stream.next().await {
    println!("Real-time metrics: {:?}", metrics);
}
```

## 🔧 Best Practices

### Application Integration

1. **Lifecycle Management**
   ```rust
   // Application startup
   let db = EmbeddedVectorDB::new(config).await?;
   
   // Register shutdown hook
   tokio::spawn(async move {
       tokio::signal::ctrl_c().await.unwrap();
       db.close().await.unwrap();
   });
   ```

2. **Error Handling**
   ```rust
   match db.search(query_vector, 10, None).await {
       Ok(results) => process_results(results),
       Err(VectorDbError::IndexNotFound) => rebuild_index().await?,
       Err(VectorDbError::InsufficientMemory) => increase_memory_limit().await?,
       Err(e) => log::error!("Search failed: {}", e),
   }
   ```

3. **Batch Operations**
   ```rust
   // Prefer batch operations for better performance
   let batch_size = 1000;
   for chunk in documents.chunks(batch_size) {
       db.batch_add_documents(chunk.to_vec()).await?;
   }
   ```

### Performance Tuning

1. **Memory Optimization**
   - Set appropriate memory limits based on available RAM
   - Use binary quantization for large-scale deployments
   - Enable memory mapping for large datasets

2. **Index Tuning**
   - Adjust HNSW parameters based on dataset size and query patterns
   - Use higher `ef_construction` for better accuracy
   - Use higher `ef_search` for better recall

3. **Caching**
   - Monitor cache hit ratios and adjust cache sizes accordingly
   - Use appropriate eviction policies based on access patterns

## 🐛 Troubleshooting

### Common Issues

#### Out of Memory Errors
```rust
// Solution: Increase memory limit or enable quantization
let config = EmbeddedConfig::default()
    .memory_limit_mb(2048)
    .enable_binary_quantization(true);
```

#### Slow Search Performance
```rust
// Solution: Tune HNSW parameters
let config = EmbeddedConfig::default()
    .hnsw_ef_search(128) // Increase for better accuracy
    .enable_caching(true);
```

#### Index Corruption
```rust
// Solution: Rebuild index
db.rebuild_index().await?;
```

### Debug Logging

```rust
// Enable debug logging
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

// Or use tracing
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

## 📈 Scaling Considerations

### When to Use Embedded Mode

- ✅ **Good for**: Desktop apps, mobile apps, edge devices, prototyping
- ✅ **Document count**: < 1 million documents
- ✅ **Memory**: < 4GB RAM available
- ✅ **Queries**: < 10,000 QPS

### Migration Path

When you outgrow embedded mode:

1. **Single Node**: Use `single_node_server.rs` example
2. **Cluster Mode**: Use `cluster_3node_simple.rs` example
3. **Cloud Deployment**: Use Kubernetes deployment guides

## 📋 Example Applications

### Document Search Engine

```rust
// Simple document search implementation
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddedConfig::default()
        .with_data_dir("./doc_search")
        .with_vector_dimension(384);
    
    let mut db = EmbeddedVectorDB::new(config).await?;
    
    // Index documents
    let documents = load_documents_from_disk().await?;
    for doc in documents {
        let embedding = generate_embedding(&doc.content).await?;
        
        db.upsert_point(Point {
            id: doc.id,
            vector: embedding,
            payload: [
                ("title".to_string(), doc.title.into()),
                ("content".to_string(), doc.content.into()),
            ].into(),
        }).await?;
    }
    
    // Search interface
    loop {
        let query = read_user_input().await?;
        let query_embedding = generate_embedding(&query).await?;
        
        let results = db.search(query_embedding, 5, None).await?;
        
        for result in results {
            println!("📄 {} (score: {:.3})", 
                result.payload["title"], result.score);
        }
    }
}
```

### Recommendation System

```rust
// User-item recommendation system
async fn recommend_items(
    db: &EmbeddedVectorDB,
    user_id: &str,
    num_recommendations: usize,
) -> Result<Vec<Item>, VectorDbError> {
    // Get user profile vector
    let user_vector = get_user_profile_vector(user_id).await?;
    
    // Search for similar items
    let filter = Filter::new()
        .must_not("user_id", user_id) // Exclude user's own items
        .must("category", "product");
    
    let results = db.search(user_vector, num_recommendations, Some(filter)).await?;
    
    // Convert to items
    let items: Vec<Item> = results
        .into_iter()
        .map(|result| Item::from_payload(result.payload))
        .collect();
    
    Ok(items)
}
```

---

This guide provides comprehensive coverage of the embedded mode deployment for Grape Vector Database. For more advanced scenarios, see the [Single Node](./deployment-single-node.md) or [3-Node Cluster](./deployment-cluster-3node.md) deployment guides.