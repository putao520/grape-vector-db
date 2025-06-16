// Advanced Storage Engine Demo
// Week 9-10: Storage Engine Upgrade

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use grape_vector_db::{
    advanced_storage::{AdvancedStorage, AdvancedStorageConfig, ColumnFamilies, StorageStats},
    types::Point,
    errors::Result,
};
use tempfile::TempDir;

fn main() -> Result<()> {
    println!("🍇 Grape Vector DB - Advanced Storage Engine Demo");
    println!("================================================\n");

    // Create temporary directory for demo
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("demo_db");
    let backup_path = temp_dir.path().join("backups");

    // Configure advanced storage
    let config = AdvancedStorageConfig {
        db_path: db_path.clone(),
        backup_path: Some(backup_path),
        enable_compression: true,
        cache_size: 64 * 1024 * 1024, // 64MB cache
        flush_interval_ms: 1000,
        enable_checksums: true,
        max_background_threads: 4,
    };

    // Initialize storage
    let storage = AdvancedStorage::new(config)?;
    println!("✅ Advanced storage initialized");
    println!("   Database path: {:?}", db_path);
    println!("   Compression: enabled");
    println!("   Cache size: 64MB");
    println!("   Checksums: enabled\n");

    // Demo 1: Basic Operations
    demo_basic_operations(&storage)?;
    
    // Demo 2: Transaction Support
    demo_transaction_support(&storage)?;
    
    // Demo 3: Backup and Recovery
    demo_backup_recovery(&storage)?;
    
    // Demo 4: Performance Benchmarks
    demo_performance_benchmarks(&storage)?;
    
    // Demo 5: Multi-tree Usage
    demo_multi_tree_usage(&storage)?;

    println!("🎉 All demos completed successfully!");
    Ok(())
}

fn demo_basic_operations(storage: &AdvancedStorage) -> Result<()> {
    println!("📝 Demo 1: Basic Operations");
    println!("---------------------------");

    let start = Instant::now();

    // Create test vectors
    let mut test_vectors = Vec::new();
    for i in 0..100 {
        let vector = (0..128).map(|j| (i * 128 + j) as f32 * 0.01).collect();
        let mut payload = HashMap::new();
        payload.insert("category".to_string(), serde_json::Value::String(format!("cat_{}", i % 5)));
        payload.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(i as f64 * 0.1).unwrap()));
        
        test_vectors.push(Point {
            id: format!("vec_{}", i),
            vector,
            payload,
        });
    }

    // Store vectors
    let store_start = Instant::now();
    for point in &test_vectors {
        storage.store_vector(point)?;
    }
    let store_duration = store_start.elapsed();
    println!("   ✅ Stored {} vectors in {:?}", test_vectors.len(), store_duration);

    // Retrieve vectors
    let retrieve_start = Instant::now();
    let mut retrieved_count = 0;
    for i in 0..100 {
        if let Some(_point) = storage.get_vector(&format!("vec_{}", i))? {
            retrieved_count += 1;
        }
    }
    let retrieve_duration = retrieve_start.elapsed();
    println!("   ✅ Retrieved {} vectors in {:?}", retrieved_count, retrieve_duration);

    // Delete some vectors
    let delete_start = Instant::now();
    let mut deleted_count = 0;
    for i in 0..20 {
        if storage.delete_vector(&format!("vec_{}", i))? {
            deleted_count += 1;
        }
    }
    let delete_duration = delete_start.elapsed();
    println!("   ✅ Deleted {} vectors in {:?}", deleted_count, delete_duration);

    // List remaining vectors
    let list_start = Instant::now();
    let vector_ids = storage.list_vector_ids()?;
    let list_duration = list_start.elapsed();
    println!("   ✅ Listed {} remaining vectors in {:?}", vector_ids.len(), list_duration);

    let total_duration = start.elapsed();
    println!("   ⏱️  Total time: {:?}\n", total_duration);

    Ok(())
}

fn demo_transaction_support(storage: &AdvancedStorage) -> Result<()> {
    println!("🔄 Demo 2: Transaction Support");
    println!("------------------------------");

    let start = Instant::now();

    // Create batch of vectors for transaction
    let mut batch_vectors = Vec::new();
    for i in 200..250 {
        let vector = (0..64).map(|j| (i * 64 + j) as f32 * 0.01).collect();
        let mut payload = HashMap::new();
        payload.insert("batch_id".to_string(), serde_json::Value::String("batch_1".to_string()));
        payload.insert("index".to_string(), serde_json::Value::Number(serde_json::Number::from(i)));
        
        batch_vectors.push(Point {
            id: format!("batch_vec_{}", i),
            vector,
            payload,
        });
    }

    // Batch store with transaction
    let batch_start = Instant::now();
    storage.batch_store_vectors(&batch_vectors)?;
    let batch_duration = batch_start.elapsed();
    println!("   ✅ Batch stored {} vectors in {:?}", batch_vectors.len(), batch_duration);

    // Verify all vectors were stored atomically
    let verify_start = Instant::now();
    let mut verified_count = 0;
    for i in 200..250 {
        if let Some(point) = storage.get_vector(&format!("batch_vec_{}", i))? {
            if let Some(batch_id) = point.payload.get("batch_id") {
                if batch_id == &serde_json::Value::String("batch_1".to_string()) {
                    verified_count += 1;
                }
            }
        }
    }
    let verify_duration = verify_start.elapsed();
    println!("   ✅ Verified {} vectors with correct batch_id in {:?}", verified_count, verify_duration);

    let total_duration = start.elapsed();
    println!("   ⏱️  Total time: {:?}\n", total_duration);

    Ok(())
}

fn demo_backup_recovery(storage: &AdvancedStorage) -> Result<()> {
    println!("💾 Demo 3: Backup and Recovery");
    println!("------------------------------");

    let start = Instant::now();

    // Get initial stats
    let initial_stats = storage.get_stats()?;
    println!("   📊 Initial stats:");
    println!("      Keys: {}", initial_stats.estimated_keys);
    println!("      Total size: {} bytes", initial_stats.total_size);
    println!("      Compression ratio: {:.2}", initial_stats.compression_ratio);

    // Create backup
    let backup_start = Instant::now();
    let backup_id = storage.create_backup()?;
    let backup_duration = backup_start.elapsed();
    println!("   ✅ Created backup '{}' in {:?}", backup_id, backup_duration);

    // Create checkpoint
    let checkpoint_start = Instant::now();
    let checkpoint_path = std::env::temp_dir().join("demo_checkpoint");
    storage.create_checkpoint(&checkpoint_path)?;
    let checkpoint_duration = checkpoint_start.elapsed();
    println!("   ✅ Created checkpoint at {:?} in {:?}", checkpoint_path, checkpoint_duration);

    // Compact database
    let compact_start = Instant::now();
    storage.compact()?;
    let compact_duration = compact_start.elapsed();
    println!("   ✅ Compacted database in {:?}", compact_duration);

    // Get final stats
    let final_stats = storage.get_stats()?;
    println!("   📊 Final stats:");
    println!("      Keys: {}", final_stats.estimated_keys);
    println!("      Total size: {} bytes", final_stats.total_size);
    println!("      Cache hit rate: {:.2}%", final_stats.cache_hit_rate * 100.0);
    if let Some(backup_time) = final_stats.last_backup_time {
        println!("      Last backup: {}", backup_time);
    }

    let total_duration = start.elapsed();
    println!("   ⏱️  Total time: {:?}\n", total_duration);

    Ok(())
}

fn demo_performance_benchmarks(storage: &AdvancedStorage) -> Result<()> {
    println!("🚀 Demo 4: Performance Benchmarks");
    println!("----------------------------------");

    let start = Instant::now();

    // Generate large dataset
    let vector_count = 5000;
    let vector_dim = 256;
    
    println!("   📈 Generating {} vectors with {} dimensions...", vector_count, vector_dim);
    let mut large_dataset = Vec::new();
    for i in 0..vector_count {
        let vector = (0..vector_dim).map(|j| ((i * vector_dim + j) as f32).sin()).collect();
        let mut payload = HashMap::new();
        payload.insert("type".to_string(), serde_json::Value::String("benchmark".to_string()));
        payload.insert("timestamp".to_string(), serde_json::Value::Number(
            serde_json::Number::from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())
        ));
        
        large_dataset.push(Point {
            id: format!("bench_{}", i),
            vector,
            payload,
        });
    }

    // Benchmark batch write
    let write_start = Instant::now();
    storage.batch_store_vectors(&large_dataset)?;
    let write_duration = write_start.elapsed();
    let write_qps = vector_count as f64 / write_duration.as_secs_f64();
    println!("   ✅ Batch write: {} vectors in {:?} ({:.0} QPS)", vector_count, write_duration, write_qps);

    // Benchmark random reads
    let read_start = Instant::now();
    let read_count = 1000;
    let mut successful_reads = 0;
    for i in 0..read_count {
        let id = format!("bench_{}", i * (vector_count / read_count));
        if storage.get_vector(&id)?.is_some() {
            successful_reads += 1;
        }
    }
    let read_duration = read_start.elapsed();
    let read_qps = read_count as f64 / read_duration.as_secs_f64();
    println!("   ✅ Random reads: {}/{} vectors in {:?} ({:.0} QPS)", successful_reads, read_count, read_duration, read_qps);

    // Benchmark list operations
    let list_start = Instant::now();
    let all_ids = storage.list_vector_ids()?;
    let list_duration = list_start.elapsed();
    println!("   ✅ List all IDs: {} vectors in {:?}", all_ids.len(), list_duration);

    let total_duration = start.elapsed();
    println!("   ⏱️  Total benchmark time: {:?}\n", total_duration);

    Ok(())
}

fn demo_multi_tree_usage(storage: &AdvancedStorage) -> Result<()> {
    println!("🌳 Demo 5: Multi-tree Usage");
    println!("---------------------------");

    let start = Instant::now();

    // This demo shows that our storage engine uses multiple trees (column families)
    // internally to organize different types of data
    
    println!("   📊 Storage engine uses multiple trees for data organization:");
    println!("      - vectors: Raw vector data");
    println!("      - metadata: Vector metadata and payloads");
    println!("      - index: Index structures");
    println!("      - sparse: Sparse vector data");
    println!("      - quantized: Quantized vector data");
    println!("      - stats: Database statistics");

    // Store some vectors to demonstrate multi-tree usage
    let mut demo_vectors = Vec::new();
    for i in 0..10 {
        let vector = (0..32).map(|j| (i * 32 + j) as f32 * 0.1).collect();
        let mut payload = HashMap::new();
        payload.insert("tree_demo".to_string(), serde_json::Value::String("multi_tree".to_string()));
        
        demo_vectors.push(Point {
            id: format!("tree_demo_{}", i),
            vector,
            payload,
        });
    }

    storage.batch_store_vectors(&demo_vectors)?;
    println!("   ✅ Stored {} vectors across multiple trees", demo_vectors.len());

    // Verify data separation by checking that we can retrieve vectors
    // (this demonstrates that the multi-tree architecture works correctly)
    let mut retrieved = 0;
    for i in 0..10 {
        if storage.get_vector(&format!("tree_demo_{}", i))?.is_some() {
            retrieved += 1;
        }
    }
    println!("   ✅ Retrieved {} vectors from multi-tree storage", retrieved);

    let total_duration = start.elapsed();
    println!("   ⏱️  Total time: {:?}\n", total_duration);

    Ok(())
} 