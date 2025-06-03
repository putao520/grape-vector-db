//! 使用OpenAI兼容API的向量数据库示例
//! 
//! 这个示例展示了如何使用不同的嵌入提供商创建向量数据库。

use grape_vector_db::*;
use std::env;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("🍇 Grape Vector Database - 嵌入提供商示例");

    // 示例1：使用Mock提供商（默认，无需API密钥）
    println!("\n📖 示例1: 使用Mock提供商");
    let mut mock_db = VectorDatabase::new("./data/mock").await?;
    
    let doc1 = Document {
        id: "rust_doc".to_string(),
        content: "Rust是一种系统编程语言，专注于安全、速度和并发性。".to_string(),
        title: Some("Rust编程语言".to_string()),
        language: Some("zh".to_string()),
        ..Default::default()
    };
    
    mock_db.add_document(doc1).await?;
    let results = mock_db.search("编程语言", 5).await?;
    println!("✅ Mock提供商搜索结果: {} 个", results.len());

    // 示例2：使用OpenAI API（如果提供了API密钥）
    if let Ok(api_key) = env::var("OPENAI_API_KEY") {
        println!("\n🤖 示例2: 使用OpenAI API");
        let mut openai_db = VectorDatabase::with_openai_compatible(
            "./data/openai",
            "https://api.openai.com/v1/embeddings".to_string(),
            api_key,
            "text-embedding-3-small".to_string()
        ).await?;
        
        let doc2 = Document {
            id: "ai_doc".to_string(),
            content: "人工智能是计算机科学的一个分支，致力于创建能够模拟人类智能的机器。".to_string(),
            title: Some("人工智能介绍".to_string()),
            language: Some("zh".to_string()),
            ..Default::default()
        };
        
        openai_db.add_document(doc2).await?;
        let results = openai_db.search("机器学习", 5).await?;
        println!("✅ OpenAI提供商搜索结果: {} 个", results.len());
    } else {
        println!("\n⏭️  跳过OpenAI示例 (未设置OPENAI_API_KEY环境变量)");
    }

    // 示例3：使用Azure OpenAI（如果提供了配置）
    if let (Ok(azure_endpoint), Ok(azure_key), Ok(deployment)) = (
        env::var("AZURE_OPENAI_ENDPOINT"),
        env::var("AZURE_OPENAI_KEY"),
        env::var("AZURE_OPENAI_DEPLOYMENT")
    ) {
        println!("\n☁️  示例3: 使用Azure OpenAI");
        let mut azure_db = VectorDatabase::with_azure_openai(
            "./data/azure",
            azure_endpoint,
            azure_key,
            deployment,
            Some("2023-05-15".to_string())
        ).await?;
        
        let doc3 = Document {
            id: "cloud_doc".to_string(),
            content: "云计算是通过互联网提供计算服务的模式。".to_string(),
            title: Some("云计算概述".to_string()),
            language: Some("zh".to_string()),
            ..Default::default()
        };
        
        azure_db.add_document(doc3).await?;
        let results = azure_db.search("云服务", 5).await?;
        println!("✅ Azure提供商搜索结果: {} 个", results.len());
    } else {
        println!("\n⏭️  跳过Azure示例 (未设置Azure配置环境变量)");
    }

    // 示例4：使用本地Ollama（如果运行中）
    println!("\n🏠 示例4: 尝试使用本地Ollama");
    match VectorDatabase::with_ollama(
        "./data/ollama",
        Some("http://localhost:11434".to_string()),
        "nomic-embed-text".to_string()
    ).await {
        Ok(mut ollama_db) => {
            let doc4 = Document {
                id: "local_doc".to_string(),
                content: "本地部署的大语言模型可以提供隐私保护和自主控制。".to_string(),
                title: Some("本地LLM".to_string()),
                language: Some("zh".to_string()),
                ..Default::default()
            };
            
            // 尝试添加文档（可能失败如果Ollama未运行）
            match ollama_db.add_document(doc4).await {
                Ok(_) => {
                    let results = ollama_db.search("本地模型", 5).await?;
                    println!("✅ Ollama提供商搜索结果: {} 个", results.len());
                },
                Err(e) => println!("⚠️  Ollama服务可能未运行: {}", e),
            }
        },
        Err(e) => println!("⚠️  无法连接到Ollama: {}", e),
    }

    // 示例5：使用自定义配置
    println!("\n⚙️  示例5: 使用自定义配置");
    let mut config = VectorDbConfig::default();
    config.embedding.provider = "mock".to_string();
    config.embedding.dimension = Some(384); // 自定义维度
    config.vector_dimension = 384;
    
    let mut custom_db = VectorDatabase::with_config("./data/custom", config).await?;
    
    let doc5 = Document {
        id: "custom_doc".to_string(),
        content: "自定义配置允许灵活调整向量数据库的行为。".to_string(),
        title: Some("自定义配置".to_string()),
        language: Some("zh".to_string()),
        ..Default::default()
    };
    
    custom_db.add_document(doc5).await?;
    let stats = custom_db.stats();
    println!("✅ 自定义配置数据库统计: {} 个文档, {} 维向量", 
             stats.document_count, stats.vector_count);

    println!("\n🎉 所有示例完成！");
    
    // 环境变量使用提示
    println!("\n💡 提示：要测试真实的嵌入提供商，请设置以下环境变量:");
    println!("   OPENAI_API_KEY=your-openai-api-key");
    println!("   AZURE_OPENAI_ENDPOINT=https://your-resource.openai.azure.com");
    println!("   AZURE_OPENAI_KEY=your-azure-key");
    println!("   AZURE_OPENAI_DEPLOYMENT=your-deployment-name");
    println!("   或启动本地Ollama服务");

    Ok(())
} 