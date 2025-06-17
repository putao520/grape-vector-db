use grape_vector_db::{
    VectorDatabase, VectorDbConfig, Document,
    errors::Result,
};
use std::time::Instant;
use std::path::PathBuf;
use tokio;
use chrono;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    println!("🔍 Storage Layer Performance Analysis");
    println!("===================================");
    
    let config = VectorDbConfig::default();
    let data_dir = PathBuf::from("./storage_test_data");
    let _ = std::fs::remove_dir_all(&data_dir);
    
    let db = VectorDatabase::new(data_dir, config).await?;
    
    // Test 1: Documents without vectors
    test_documents_without_vectors(&db).await?;
    
    // Test 2: Documents with vectors
    test_documents_with_vectors(&db).await?;
    
    Ok(())
}

async fn test_documents_without_vectors(db: &VectorDatabase) -> Result<()> {
    println!("📋 测试1: 50个无向量文档的批量插入");
    
    let mut documents = Vec::new();
    for i in 0..50 {
        let doc = Document {
            id: format!("no_vector_doc_{}", i),
            content: format!("这是无向量测试文档 {} 的内容。", i),
            title: Some(format!("无向量测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("test_type".to_string(), "no_vector".to_string());
                meta
            },
            vector: None, // 不包含向量
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        documents.push(doc);
    }
    
    let start = Instant::now();
    let _ids = db.batch_add_documents(documents).await?;
    let elapsed = start.elapsed();
    
    println!("  ⏱️  耗时: {:?}", elapsed);
    println!("  📊 性能: {:.2} docs/sec", 50.0 / elapsed.as_secs_f64());
    
    if elapsed.as_secs_f64() < 0.1 {
        println!("  ✅ 存储层性能优秀!");
    } else if elapsed.as_secs_f64() < 1.0 {
        println!("  🟡 存储层性能良好");
    } else {
        println!("  ❌ 存储层性能有问题");
    }
    
    println!();
    Ok(())
}

async fn test_documents_with_vectors(db: &VectorDatabase) -> Result<()> {
    println!("🎯 测试2: 50个带向量文档的批量插入");
    
    let mut documents = Vec::new();
    for i in 0..50 {
        let doc = Document {
            id: format!("with_vector_doc_{}", i),
            content: format!("这是带向量测试文档 {} 的内容。", i),
            title: Some(format!("带向量测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("test".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("test_type".to_string(), "with_vector".to_string());
                meta
            },
            vector: Some(vec![0.1; 384]), // 简单的固定向量，避免随机生成开销
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        documents.push(doc);
    }
    
    let start = Instant::now();
    let _ids = db.batch_add_documents(documents).await?;
    let elapsed = start.elapsed();
    
    println!("  ⏱️  耗时: {:?}", elapsed);
    println!("  📊 性能: {:.2} docs/sec", 50.0 / elapsed.as_secs_f64());
    
    if elapsed.as_secs_f64() < 1.0 {
        println!("  ✅ 向量索引性能优秀!");
    } else if elapsed.as_secs_f64() < 5.0 {
        println!("  🟡 向量索引性能一般");
    } else {
        println!("  ❌ 向量索引性能瓶颈!");
    }
    
    println!();
    Ok(())
}