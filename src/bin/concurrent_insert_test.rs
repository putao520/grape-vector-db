use chrono;
use grape_vector_db::{errors::Result, Document, VectorDatabase, VectorDbConfig};
use rand;
use std::path::PathBuf;
use std::time::Instant;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("⚡ Grape Vector DB 并发插入测试 - 解决50文档4-5秒的性能问题");
    println!("============================================================");

    // 创建数据库配置
    let config = VectorDbConfig::default();
    let data_dir = PathBuf::from("./concurrent_test_data");

    // 清理旧数据
    let _ = std::fs::remove_dir_all(&data_dir);

    let db = VectorDatabase::new(data_dir, config).await?;

    println!("🚀 测试场景: 并发插入50个文档 (企业级要求 < 1秒)");
    println!();

    // 测试不同的插入策略
    test_batch_insertion(&db).await?;
    test_sequential_insertion(&db).await?;

    Ok(())
}

async fn test_batch_insertion(db: &VectorDatabase) -> Result<()> {
    println!("📋 测试1: 批量插入50个文档");

    // 准备50个测试文档
    let mut documents = Vec::new();
    for i in 0..50 {
        let doc = Document {
            id: format!("batch_test_doc_{}", i),
            content: format!(
                "这是批量测试文档 {} 的内容。包含向量数据用于测试批量插入性能。",
                i
            ),
            title: Some(format!("批量测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("batch_test".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("test_type".to_string(), "batch".to_string());
                meta.insert("doc_index".to_string(), i.to_string());
                meta
            },
            vector: Some((0..384).map(|_| rand::random::<f32>()).collect()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        documents.push(doc);
    }

    let start = Instant::now();
    let _ids = db.batch_add_documents(documents).await?;
    let elapsed = start.elapsed();

    println!("  ⏱️  批量插入耗时: {:?}", elapsed);
    println!(
        "  📊 性能指标: {:.2} docs/sec",
        50.0 / elapsed.as_secs_f64()
    );

    if elapsed.as_secs_f64() < 1.0 {
        println!("  ✅ 性能优秀! (< 1秒，满足企业级要求)");
    } else if elapsed.as_secs_f64() < 2.0 {
        println!("  🟡 性能良好 (< 2秒)");
    } else if elapsed.as_secs_f64() < 5.0 {
        println!("  🟠 性能一般 (< 5秒)");
    } else {
        println!("  ❌ 性能差 (>= 5秒，不满足企业级要求)");
    }

    println!();
    Ok(())
}

async fn test_sequential_insertion(db: &VectorDatabase) -> Result<()> {
    println!("🔄 测试2: 顺序插入50个文档 (对比测试)");

    let start = Instant::now();

    for i in 0..50 {
        let doc = Document {
            id: format!("sequential_test_doc_{}", i),
            content: format!("这是顺序测试文档 {} 的内容。", i),
            title: Some(format!("顺序测试文档 {}", i)),
            language: Some("zh".to_string()),
            package_name: Some("sequential_test".to_string()),
            version: Some("1.0.0".to_string()),
            doc_type: Some("test".to_string()),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("test_type".to_string(), "sequential".to_string());
                meta.insert("doc_index".to_string(), i.to_string());
                meta
            },
            vector: Some((0..384).map(|_| rand::random::<f32>()).collect()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let _id = db.add_document(doc).await?;
    }

    let elapsed = start.elapsed();

    println!("  ⏱️  顺序插入耗时: {:?}", elapsed);
    println!(
        "  📊 性能指标: {:.2} docs/sec",
        50.0 / elapsed.as_secs_f64()
    );

    if elapsed.as_secs_f64() < 1.0 {
        println!("  ✅ 性能优秀! (< 1秒)");
    } else if elapsed.as_secs_f64() < 2.0 {
        println!("  🟡 性能良好 (< 2秒)");
    } else if elapsed.as_secs_f64() < 5.0 {
        println!("  🟠 性能一般 (< 5秒)");
    } else {
        println!("  ❌ 性能差 (>= 5秒)");
    }

    println!();
    Ok(())
}
