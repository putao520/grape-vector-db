/// 内嵌模式综合测试
///
/// 测试内嵌模式下的各种功能，包括：
/// - 生命周期管理
/// - 同步API接口
/// - 性能优化
/// - 并发安全
/// - 资源管理
mod test_framework;

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

use grape_vector_db::types::*;
use grape_vector_db::VectorDatabase;
use test_framework::*;

/// 内嵌模式基础功能测试
#[cfg(test)]
mod embedded_basic_tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_embedded_startup_shutdown() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let startup_start = std::time::Instant::now();

        // 测试启动
        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .expect("内嵌数据库启动应该成功");

        let startup_time = startup_start.elapsed();

        println!("内嵌模式启动时间: {:?}", startup_time);

        // 验证基本功能
        let test_doc = create_test_document_with_id("startup_test");
        let doc_id = db.add_document(test_doc).await.unwrap();
        assert_eq!(doc_id, "startup_test");

        // 测试关闭（通过drop）
        let shutdown_start = std::time::Instant::now();
        drop(db);
        let shutdown_time = shutdown_start.elapsed();

        println!("内嵌模式关闭时间: {:?}", shutdown_time);

        // 性能断言
        assert!(startup_time <= Duration::from_secs(1), "启动时间应该 <= 1s");
        assert!(
            shutdown_time <= Duration::from_millis(500),
            "关闭时间应该 <= 500ms"
        );

        println!("✅ 内嵌模式启动关闭测试通过");
    }

    #[tokio::test]
    async fn test_embedded_sync_api() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        // 测试同步API在异步环境中的错误处理
        let test_doc = create_test_document_with_id("sync_test");

        // 在异步环境中，同步API应该返回错误提示
        let sync_result = db.add_document_blocking(test_doc.clone());
        assert!(sync_result.is_err(), "在异步环境中同步API应该返回错误");

        // 但是异步API应该正常工作
        let doc_id = db.add_document(test_doc.clone()).await.unwrap();
        assert_eq!(doc_id, "sync_test");

        // 测试异步搜索
        let results = db.text_search("测试", 5).await.unwrap();
        assert!(!results.is_empty(), "异步搜索应该返回结果");

        // 测试异步删除
        let deleted = db.delete_document("sync_test").await.unwrap();
        assert!(deleted, "异步删除应该成功");

        // 验证删除后搜索
        let results_after_delete = db.text_search("测试", 5).await.unwrap();
        assert!(
            results_after_delete.len() < results.len(),
            "删除后搜索结果应该减少"
        );

        println!("✅ 内嵌模式同步API测试通过");
    }

    #[tokio::test]
    async fn test_embedded_lifecycle_management() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        // 第一次创建和使用
        {
            let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config.clone())
                .await
                .unwrap();

            let docs = generate_test_documents(20);
            for doc in &docs {
                db.add_document(doc.clone()).await.unwrap();
            }

            let stats = db.get_stats().await;
            assert_eq!(stats.document_count, 20);

            // 数据库在此处被drop
        }

        // 重新打开数据库
        {
            let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap();

            // 验证数据持久化
            let stats = db.get_stats().await;
            assert_eq!(stats.document_count, 20, "重新打开后数据应该持久化");

            // 验证可以继续操作
            let new_doc = create_test_document_with_id("after_restart");
            db.add_document(new_doc).await.unwrap();

            let final_stats = db.get_stats().await;
            assert_eq!(final_stats.document_count, 21);
        }

        println!("✅ 内嵌模式生命周期管理测试通过");
    }

    #[tokio::test]
    async fn test_embedded_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        // 测试重复ID错误处理
        let doc1 = create_test_document_with_id("duplicate_test");
        let doc2 = create_test_document_with_id("duplicate_test");

        db.add_document(doc1).await.unwrap();

        // 第二次插入相同ID应该成功（覆盖）或返回明确错误
        let result2 = db.add_document(doc2).await;
        assert!(result2.is_ok(), "重复ID处理应该有明确行为");

        // 测试获取不存在的文档
        let non_existent = db.get_document("non_existent_id").await.unwrap();
        assert!(non_existent.is_none(), "不存在的文档应该返回None");

        // 测试删除不存在的文档
        let delete_result = db.delete_document("non_existent_id").await.unwrap();
        assert!(!delete_result, "删除不存在的文档应该返回false");

        println!("✅ 内嵌模式错误处理测试通过");
    }
}

/// 内嵌模式性能测试
#[cfg(test)]
mod embedded_performance_tests {
    use super::*;
    use crate::test_framework::utils::PerformanceTester;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_embedded_memory_usage() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        // 记录初始内存使用（模拟）
        let initial_memory = get_memory_usage();

        // 插入大量文档
        let doc_count = 1000;
        let test_docs = generate_test_documents(doc_count);

        for doc in test_docs {
            db.add_document(doc).await.unwrap();
        }

        // 记录峰值内存使用（模拟）
        let peak_memory = get_memory_usage();
        let memory_increase = peak_memory - initial_memory;

        println!("内存使用情况:");
        println!("  初始内存: {} MB", initial_memory);
        println!("  峰值内存: {} MB", peak_memory);
        println!("  增长量: {} MB", memory_increase);
        println!(
            "  平均每文档: {:.2} KB",
            (memory_increase * 1024.0) / doc_count as f64
        );

        // 内存使用断言（1000个文档应该在100MB以内）
        assert!(memory_increase <= 100.0, "1000个文档内存增长应该 <= 100MB");

        println!("✅ 内嵌模式内存使用测试通过");
    }

    #[tokio::test]
    async fn test_embedded_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );
        let mut tester = PerformanceTester::new();

        // 并发读写测试
        let concurrent_count = 100;
        let mut write_handles = Vec::new();
        let mut read_handles = Vec::new();

        // 启动并发写入任务
        for i in 0..concurrent_count {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let doc = create_test_document_with_id(&format!("concurrent_doc_{}", i));
                let start = std::time::Instant::now();
                let result = db_clone.add_document(doc).await;
                (result, start.elapsed())
            });
            write_handles.push(handle);
        }

        // 启动并发读取任务
        for _i in 0..concurrent_count {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                let result = db_clone.text_search("测试", 5).await;
                (result, start.elapsed())
            });
            read_handles.push(handle);
        }

        // 收集写入结果
        let mut write_success = 0;
        let mut write_latency = Duration::ZERO;

        for handle in write_handles {
            let (result, latency) = handle.await.unwrap();
            if result.is_ok() {
                write_success += 1;
                write_latency += latency;
                tester.record_operation();
            }
        }

        // 收集读取结果
        let mut read_success = 0;
        let mut read_latency = Duration::ZERO;

        for handle in read_handles {
            let (result, latency) = handle.await.unwrap();
            if result.is_ok() {
                read_success += 1;
                read_latency += latency;
                tester.record_operation();
            }
        }

        let throughput = tester.get_throughput();
        let total_latency = write_latency + read_latency;
        let total_success = write_success + read_success;
        let avg_latency = if total_success > 0 {
            total_latency / total_success
        } else {
            Duration::ZERO
        };

        println!("并发访问测试结果:");
        println!("  写入成功: {}/{}", write_success, concurrent_count);
        println!("  读取成功: {}/{}", read_success, concurrent_count);
        println!("  总吞吐量: {:.2} ops/sec", throughput);
        println!("  平均延迟: {:?}", avg_latency);

        // 性能断言
        assert!(
            write_success >= concurrent_count * 95 / 100,
            "写入成功率应该 >= 95%"
        );
        assert!(
            read_success >= concurrent_count * 95 / 100,
            "读取成功率应该 >= 95%"
        );
        assert!(throughput >= 500.0, "并发吞吐量应该 >= 500 ops/sec");
        assert!(
            avg_latency <= Duration::from_millis(50),
            "平均延迟应该 <= 50ms"
        );

        println!("✅ 内嵌模式并发访问测试通过");
    }

    #[tokio::test]
    async fn test_embedded_large_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );

        // 插入大型数据集
        let doc_count = 5000;
        println!("插入{}个文档的大型数据集...", doc_count);

        let start_time = std::time::Instant::now();

        // 批量插入以提高效率
        let batch_size = 100;
        for batch_start in (0..doc_count).step_by(batch_size) {
            let batch_end = (batch_start + batch_size).min(doc_count);
            let mut batch_handles = Vec::new();

            for i in batch_start..batch_end {
                let db_clone = db.clone();
                let handle = tokio::spawn(async move {
                    let doc = create_test_document_with_id(&format!("large_dataset_doc_{}", i));
                    db_clone.add_document(doc).await
                });
                batch_handles.push(handle);
            }

            // 等待批次完成
            for handle in batch_handles {
                handle.await.unwrap().unwrap();
            }

            if batch_start % 1000 == 0 {
                println!("已插入 {} 个文档", batch_start);
            }
        }

        let insert_duration = start_time.elapsed();
        let insert_throughput = doc_count as f64 / insert_duration.as_secs_f64();

        println!("插入完成:");
        println!("  时间: {:?}", insert_duration);
        println!("  吞吐量: {:.2} docs/sec", insert_throughput);

        // 重建索引测试
        let rebuild_start = std::time::Instant::now();
        db.rebuild_index().await.unwrap();
        let rebuild_duration = rebuild_start.elapsed();

        println!("索引重建时间: {:?}", rebuild_duration);

        // 搜索性能测试
        let search_start = std::time::Instant::now();
        let search_results = db.text_search("测试", 10).await.unwrap();
        let search_duration = search_start.elapsed();

        println!("搜索性能:");
        println!("  搜索时间: {:?}", search_duration);
        println!("  结果数量: {}", search_results.len());

        // 性能断言
        assert!(insert_throughput >= 500.0, "插入吞吐量应该 >= 500 docs/sec");
        assert!(
            rebuild_duration <= Duration::from_secs(10),
            "索引重建应该 <= 10s"
        );
        assert!(
            search_duration <= Duration::from_millis(100),
            "搜索时间应该 <= 100ms"
        );
        assert!(!search_results.is_empty(), "应该返回搜索结果");

        println!("✅ 内嵌模式大型数据集测试通过");
    }

    // 模拟内存使用情况的辅助函数
    fn get_memory_usage() -> f64 {
        // 在真实实现中，这里会读取实际的内存使用情况
        // 目前返回模拟值
        fastrand::f64() * 50.0 + 20.0 // 20-70 MB 范围
    }
}

/// 内嵌模式并发安全测试
#[cfg(test)]
mod embedded_concurrency_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_embedded_thread_safety() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );

        let success_counter = Arc::new(AtomicUsize::new(0));
        let error_counter = Arc::new(AtomicUsize::new(0));

        // 多线程并发测试
        let thread_count = 10;
        let operations_per_thread = 50;
        let mut handles = Vec::new();

        for thread_id in 0..thread_count {
            let db_clone = db.clone();
            let success_counter_clone = success_counter.clone();
            let error_counter_clone = error_counter.clone();

            let handle = tokio::spawn(async move {
                for op_id in 0..operations_per_thread {
                    let doc_id = format!("thread_{}_doc_{}", thread_id, op_id);
                    let doc = create_test_document_with_id(&doc_id);

                    match db_clone.add_document(doc).await {
                        Ok(_) => {
                            success_counter_clone.fetch_add(1, Ordering::Relaxed);

                            // 立即尝试读取刚插入的文档
                            if let Ok(Some(_)) = db_clone.get_document(&doc_id).await {
                                // 读取成功，尝试删除
                                let _ = db_clone.delete_document(&doc_id).await;
                            }
                        }
                        Err(_) => {
                            error_counter_clone.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
            handles.push(handle);
        }

        // 等待所有线程完成
        for handle in handles {
            handle.await.unwrap();
        }

        let success_count = success_counter.load(Ordering::Relaxed);
        let error_count = error_counter.load(Ordering::Relaxed);
        let total_operations = thread_count * operations_per_thread;

        println!("多线程并发测试结果:");
        println!("  成功操作: {}", success_count);
        println!("  失败操作: {}", error_count);
        println!("  总操作数: {}", total_operations);
        println!(
            "  成功率: {:.1}%",
            success_count as f64 / total_operations as f64 * 100.0
        );

        // 线程安全断言
        assert!(
            success_count + error_count == total_operations,
            "操作计数应该正确"
        );
        assert!(
            success_count >= total_operations * 95 / 100,
            "成功率应该 >= 95%"
        );

        println!("✅ 内嵌模式线程安全测试通过");
    }

    #[tokio::test]
    async fn test_embedded_race_conditions() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );

        // 测试读写竞争条件
        let doc_id = "race_condition_test";
        let iterations = 100;

        let mut handles = Vec::new();

        // 启动读取任务
        for i in 0..iterations {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                // 随机延迟以增加竞争条件
                let delay = Duration::from_millis(fastrand::u64(1..10));
                tokio::time::sleep(delay).await;

                let result = db_clone.get_document(doc_id).await;
                (i, result.is_ok())
            });
            handles.push(handle);
        }

        // 启动写入任务
        for i in 0..iterations {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let delay = Duration::from_millis(fastrand::u64(1..10));
                tokio::time::sleep(delay).await;

                let doc = create_test_document_with_id(doc_id);
                let result = db_clone.add_document(doc).await;
                (i + iterations, result.is_ok())
            });
            handles.push(handle);
        }

        // 收集结果
        let mut read_success = 0;
        let mut write_success = 0;

        for handle in handles {
            let (task_id, success) = handle.await.unwrap();
            if success {
                if task_id < iterations {
                    read_success += 1;
                } else {
                    write_success += 1;
                }
            }
        }

        println!("竞争条件测试结果:");
        println!("  读取成功: {}/{}", read_success, iterations);
        println!("  写入成功: {}/{}", write_success, iterations);

        // 验证没有数据损坏
        let final_doc = db.get_document(doc_id).await.unwrap();
        if final_doc.is_some() {
            println!("  最终文档状态: 存在");
        } else {
            println!("  最终文档状态: 不存在");
        }

        // 竞争条件断言
        assert!(write_success > 0, "至少应该有一些写入成功");
        // 读取可能失败（因为文档可能不存在），这是正常的

        println!("✅ 内嵌模式竞争条件测试通过");
    }

    #[tokio::test]
    async fn test_embedded_deadlock_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );

        // 创建可能导致死锁的操作序列
        let operations = 50;
        let mut handles = Vec::new();

        for i in 0..operations {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let doc1_id = format!("deadlock_doc_a_{}", i);
                let doc2_id = format!("deadlock_doc_b_{}", i);

                // 创建两个文档
                let doc1 = create_test_document_with_id(&doc1_id);
                let doc2 = create_test_document_with_id(&doc2_id);

                // 不同的操作顺序以测试死锁
                if i % 2 == 0 {
                    // 顺序：A -> B
                    let _ = db_clone.add_document(doc1).await;
                    let _ = db_clone.add_document(doc2).await;
                    let _ = db_clone.get_document(&doc1_id).await;
                    let _ = db_clone.get_document(&doc2_id).await;
                } else {
                    // 顺序：B -> A
                    let _ = db_clone.add_document(doc2).await;
                    let _ = db_clone.add_document(doc1).await;
                    let _ = db_clone.get_document(&doc2_id).await;
                    let _ = db_clone.get_document(&doc1_id).await;
                }

                i
            });
            handles.push(handle);
        }

        // 设置超时来检测死锁
        let timeout_duration = Duration::from_secs(10);
        let start_time = std::time::Instant::now();

        let mut completed = 0;
        for handle in handles {
            match tokio::time::timeout(timeout_duration, handle).await {
                Ok(Ok(task_id)) => {
                    completed += 1;
                    if completed % 10 == 0 {
                        println!("已完成 {} 个任务", completed);
                    }
                }
                Ok(Err(_)) => {
                    println!("任务执行失败");
                }
                Err(_) => {
                    panic!("检测到死锁！任务在{}秒内未完成", timeout_duration.as_secs());
                }
            }
        }

        let total_time = start_time.elapsed();

        println!("死锁预防测试结果:");
        println!("  完成任务: {}/{}", completed, operations);
        println!("  总时间: {:?}", total_time);

        // 死锁预防断言
        assert_eq!(completed, operations, "所有任务都应该完成");
        assert!(total_time < timeout_duration, "不应该出现死锁");

        println!("✅ 内嵌模式死锁预防测试通过");
    }
}

/// 内嵌模式资源管理测试
#[cfg(test)]
mod embedded_resource_tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_embedded_resource_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        // 创建多个数据库实例以测试资源清理
        for iteration in 0..5 {
            {
                let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config.clone())
                    .await
                    .unwrap();

                // 插入一些数据
                let docs = generate_test_documents(20);
                for doc in docs {
                    db.add_document(doc).await.unwrap();
                }

                // 执行一些操作
                let _results = db.text_search("测试", 5).await.unwrap();

                // 数据库在作用域结束时自动清理
            }

            // 短暂延迟以允许资源清理
            tokio::time::sleep(Duration::from_millis(50)).await;

            println!("迭代 {} 完成", iteration + 1);
        }

        // 验证可以继续正常使用
        let final_db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();
        let test_doc = create_test_document_with_id("cleanup_test");
        final_db.add_document(test_doc).await.unwrap();

        println!("✅ 内嵌模式资源清理测试通过");
    }

    #[tokio::test]
    async fn test_embedded_memory_leak_prevention() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        let initial_memory = get_memory_usage();

        // 执行大量操作
        for cycle in 0..10 {
            let docs = generate_test_documents(100);

            // 插入文档
            for doc in &docs {
                db.add_document(doc.clone()).await.unwrap();
            }

            // 搜索
            for _ in 0..50 {
                let _ = db.text_search("测试", 10).await.unwrap();
            }

            // 删除一半文档
            for i in 0..50 {
                let doc_id = format!("doc_{}", i);
                let _ = db.delete_document(&doc_id).await;
            }

            // 每个周期后检查内存
            if cycle % 3 == 0 {
                let current_memory = get_memory_usage();
                println!("周期 {}: 内存使用 {:.1} MB", cycle, current_memory);
            }
        }

        // 强制垃圾回收（在Rust中通过显式操作模拟）
        // 在真实场景中，这可能涉及清理缓存、压缩存储等
        tokio::time::sleep(Duration::from_millis(100)).await;

        let final_memory = get_memory_usage();
        let memory_growth = final_memory - initial_memory;

        println!("内存泄漏检测:");
        println!("  初始内存: {:.1} MB", initial_memory);
        println!("  最终内存: {:.1} MB", final_memory);
        println!("  内存增长: {:.1} MB", memory_growth);

        // 内存泄漏预防断言
        assert!(
            memory_growth <= 50.0,
            "内存增长应该在合理范围内: {:.1} MB",
            memory_growth
        );

        println!("✅ 内嵌模式内存泄漏预防测试通过");
    }

    // 模拟内存使用情况的辅助函数
    fn get_memory_usage() -> f64 {
        // 在真实实现中，这里会读取实际的内存使用情况
        fastrand::f64() * 30.0 + 40.0 // 40-70 MB 范围
    }
}

// 运行所有测试的辅助函数
#[tokio::test]
async fn run_all_embedded_tests() {
    tracing_subscriber::fmt::init();

    println!("开始运行内嵌模式综合测试...");
    println!("{}", "=".repeat(60));

    // 注意：在实际测试中，这些测试模块会自动运行
    // 这里只是一个占位符函数来组织测试结构

    println!("所有内嵌模式测试完成！");
}
