use crate::{VectorDatabase, VectorDbConfig, Document, concurrent::*};
use std::sync::Arc;
use std::time::Instant;
#[cfg(test)]
use tempfile::TempDir;

/// 并发性能测试套件
#[cfg(test)]
pub struct ConcurrentPerformanceTest {
    pub db: Arc<VectorDatabase>,
    pub counters: Arc<AtomicCounters>,
    pub cache: Arc<ConcurrentHashMap<String, Vec<String>>>,
}

#[cfg(test)]
impl ConcurrentPerformanceTest {
    /// 创建新的性能测试实例
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let config = VectorDbConfig::default();
        let db = Arc::new(VectorDatabase::new(temp_dir.path().to_path_buf(), config).await?);
        
        Ok(Self {
            db,
            counters: Arc::new(AtomicCounters::new()),
            cache: Arc::new(ConcurrentHashMap::new()),
        })
    }

    /// 并发插入性能测试
    pub async fn test_concurrent_insertions(&self, batch_count: usize, batch_size: usize) -> Result<f64, Box<dyn std::error::Error>> {
        println!("🚀 测试并发插入性能：{} 批次，每批 {} 个文档", batch_count, batch_size);
        
        let start_time = Instant::now();
        let mut handles = Vec::new();
        
        for batch_id in 0..batch_count {
            let db = self.db.clone();
            let counters = self.counters.clone();
            
            let handle = tokio::spawn(async move {
                let mut documents = Vec::new();
                
                for doc_id in 0..batch_size {
                    let id = format!("batch_{}_doc_{}", batch_id, doc_id);
                    let doc = Document {
                        id: id.clone(),
                        title: Some(format!("并发测试文档 {}", id)),
                        content: format!("这是批次 {} 中的第 {} 个测试文档，用于并发性能测试", batch_id, doc_id),
                        language: Some("zh".to_string()),
                        version: Some("1".to_string()),
                        doc_type: Some("performance_test".to_string()),
                        package_name: Some("concurrent_test".to_string()),
                        vector: Some(vec![0.1 * (batch_id as f32 + doc_id as f32); 128]),
                        metadata: std::collections::HashMap::new(),
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };
                    documents.push(doc);
                }
                
                counters.increment_operations();
                let result = db.batch_add_documents(documents).await;
                
                match result {
                    Ok(_) => counters.increment_successful_operations(),
                    Err(_) => counters.increment_failed_operations(),
                }
                
                result
            });
            
            handles.push(handle);
        }
        
        // 等待所有批次完成
        let mut total_docs = 0;
        for handle in handles {
            match handle.await? {
                Ok(doc_ids) => total_docs += doc_ids.len(),
                Err(e) => eprintln!("批次插入失败: {}", e),
            }
        }
        
        let elapsed = start_time.elapsed();
        let docs_per_second = total_docs as f64 / elapsed.as_secs_f64();
        
        println!("✅ 并发插入完成:");
        println!("   总文档数: {}", total_docs);
        println!("   耗时: {:?}", elapsed);
        println!("   性能: {:.2} 文档/秒", docs_per_second);
        println!("   成功率: {:.1}%", self.counters.success_rate() * 100.0);
        
        Ok(docs_per_second)
    }

    /// 并发搜索性能测试
    pub async fn test_concurrent_searches(&self, concurrent_searches: usize, searches_per_thread: usize) -> Result<f64, Box<dyn std::error::Error>> {
        println!("🔍 测试并发搜索性能：{} 个并发线程，每线程 {} 次搜索", concurrent_searches, searches_per_thread);
        
        let queries = vec![
            "并发测试", "性能优化", "向量数据库", "文档搜索", 
            "多线程", "异步处理", "高性能", "批量操作",
            "缓存优化", "内存管理", "数据结构", "算法优化",
        ];
        
        let start_time = Instant::now();
        let mut handles = Vec::new();
        
        for thread_id in 0..concurrent_searches {
            let db = self.db.clone();
            let counters = self.counters.clone();
            let cache = self.cache.clone();
            let queries = queries.clone();
            
            let handle = tokio::spawn(async move {
                let mut successful_searches = 0;
                
                for search_id in 0..searches_per_thread {
                    let query = &queries[search_id % queries.len()];
                    let cache_key = format!("{}_{}", thread_id, search_id);
                    
                    counters.increment_search_operations();
                    
                    // 检查缓存
                    if let Some(_cached) = cache.get(&cache_key) {
                        counters.increment_cache_hits();
                        successful_searches += 1;
                        continue;
                    }
                    
                    // 执行搜索
                    match db.text_search(query, 10).await {
                        Ok(results) => {
                            // 缓存结果
                            let result_ids: Vec<String> = results.iter()
                                .map(|r| r.document.id.clone())
                                .collect();
                            cache.insert(cache_key, result_ids);
                            
                            counters.increment_cache_misses();
                            counters.increment_successful_operations();
                            successful_searches += 1;
                        },
                        Err(_) => {
                            counters.increment_failed_operations();
                        }
                    }
                }
                
                successful_searches
            });
            
            handles.push(handle);
        }
        
        // 等待所有搜索完成
        let mut total_searches = 0;
        for handle in handles {
            total_searches += handle.await?;
        }
        
        let elapsed = start_time.elapsed();
        let searches_per_second = total_searches as f64 / elapsed.as_secs_f64();
        
        println!("✅ 并发搜索完成:");
        println!("   总搜索次数: {}", total_searches);
        println!("   耗时: {:?}", elapsed);
        println!("   性能: {:.2} 搜索/秒", searches_per_second);
        println!("   缓存命中率: {:.1}%", self.counters.cache_hit_rate() * 100.0);
        println!("   成功率: {:.1}%", self.counters.success_rate() * 100.0);
        
        Ok(searches_per_second)
    }

    /// 混合工作负载测试
    pub async fn test_mixed_workload(&self, duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚡ 测试混合工作负载：持续 {} 秒", duration_secs);
        
        let end_time = Instant::now() + std::time::Duration::from_secs(duration_secs);
        let mut handles = Vec::new();
        
        // 启动插入任务
        let db_insert = self.db.clone();
        let counters_insert = self.counters.clone();
        let insert_handle = tokio::spawn(async move {
            let mut doc_counter = 0;
            while Instant::now() < end_time {
                let doc = Document {
                    id: format!("mixed_doc_{}", doc_counter),
                    title: Some(format!("混合负载文档 {}", doc_counter)),
                    content: format!("这是混合负载测试中的第 {} 个文档", doc_counter),
                    language: Some("zh".to_string()),
                    version: Some("1".to_string()),
                    doc_type: Some("mixed_test".to_string()),
                    package_name: Some("mixed_workload".to_string()),
                    vector: Some(vec![0.01 * doc_counter as f32; 128]),
                    metadata: std::collections::HashMap::new(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                
                if db_insert.add_document(doc).await.is_ok() {
                    counters_insert.increment_successful_operations();
                } else {
                    counters_insert.increment_failed_operations();
                }
                
                doc_counter += 1;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        handles.push(insert_handle);
        
        // 启动搜索任务
        let db_search = self.db.clone();
        let counters_search = self.counters.clone();
        let search_handle = tokio::spawn(async move {
            let queries = vec!["混合", "负载", "测试", "文档", "性能"];
            let mut query_counter = 0;
            
            while Instant::now() < end_time {
                let query = &queries[query_counter % queries.len()];
                
                if db_search.text_search(query, 5).await.is_ok() {
                    counters_search.increment_successful_operations();
                } else {
                    counters_search.increment_failed_operations();
                }
                
                counters_search.increment_search_operations();
                query_counter += 1;
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        });
        handles.push(search_handle);
        
        // 等待所有任务完成
        for handle in handles {
            handle.await?;
        }
        
        println!("✅ 混合工作负载测试完成:");
        println!("   总操作数: {}", self.counters.get_operations());
        println!("   成功率: {:.1}%", self.counters.success_rate() * 100.0);
        println!("   搜索操作数: {}", self.counters.search_operations.load(std::sync::atomic::Ordering::Relaxed));
        
        Ok(())
    }

    /// 运行完整的性能测试套件
    pub async fn run_full_suite(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏁 开始完整并发性能测试套件");
        println!("=====================================");
        
        // 重置计数器
        self.counters.reset();
        
        // 1. 并发插入测试
        let insert_perf = self.test_concurrent_insertions(5, 20).await?;
        println!();
        
        // 2. 并发搜索测试
        let search_perf = self.test_concurrent_searches(10, 20).await?;
        println!();
        
        // 3. 混合工作负载测试
        self.test_mixed_workload(10).await?;
        println!();
        
        // 总结
        println!("📊 性能测试总结:");
        println!("================");
        println!("   插入性能: {:.2} 文档/秒", insert_perf);
        println!("   搜索性能: {:.2} 搜索/秒", search_perf);
        println!("   缓存命中率: {:.1}%", self.counters.cache_hit_rate() * 100.0);
        println!("   总体成功率: {:.1}%", self.counters.success_rate() * 100.0);
        
        // 性能等级评估
        if insert_perf > 1000.0 && search_perf > 500.0 {
            println!("   🏆 性能等级: 优秀");
        } else if insert_perf > 500.0 && search_perf > 200.0 {
            println!("   🥈 性能等级: 良好");
        } else if insert_perf > 100.0 && search_perf > 50.0 {
            println!("   🥉 性能等级: 一般");
        } else {
            println!("   ⚠️  性能等级: 需要优化");
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_performance_suite() {
        let test_suite = ConcurrentPerformanceTest::new().await.unwrap();
        
        // 运行一个简化的测试版本
        let insert_perf = test_suite.test_concurrent_insertions(2, 10).await.unwrap();
        assert!(insert_perf > 0.0, "插入性能应该大于0");
        
        let search_perf = test_suite.test_concurrent_searches(3, 5).await.unwrap();
        assert!(search_perf > 0.0, "搜索性能应该大于0");
        
        println!("并发性能测试通过：插入 {:.2} doc/s, 搜索 {:.2} search/s", insert_perf, search_perf);
    }
}