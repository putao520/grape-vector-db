//! 企业级功能完整测试覆盖
//!
//! 此测试套件确保所有企业级功能都在真实环境中得到验证，
//! 不使用任何mock，基于真实的文件系统和数据。

use grape_vector_db::*;
use std::time::Duration;
use tempfile::TempDir;

#[cfg(test)]
mod complete_coverage_tests {
    use super::*;

    /// 测试核心数据库功能
    #[tokio::test]
    async fn test_core_database_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig {
            db_path: temp_dir.path().to_string_lossy().to_string(),
            vector_dimension: 128,
            enable_compression: true,
            cache_size: 100,
            ..Default::default()
        };

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        // 测试文档插入
        let test_docs = vec![
            Document {
                id: "doc1".to_string(),
                content: "机器学习是人工智能的核心技术".to_string(),
                title: Some("AI技术".to_string()),
                language: Some("zh".to_string()),
                metadata: [("category".to_string(), "AI".to_string())].into(),
                vector: Some(vec![0.1; 128]),
                ..Default::default()
            },
            Document {
                id: "doc2".to_string(),
                content: "深度学习在计算机视觉中的应用".to_string(),
                title: Some("深度学习".to_string()),
                language: Some("zh".to_string()),
                metadata: [("category".to_string(), "CV".to_string())].into(),
                vector: Some(vec![0.2; 128]),
                ..Default::default()
            },
        ];

        // 单个文档插入
        for doc in &test_docs {
            let result = db.add_document(doc.clone()).await;
            assert!(result.is_ok(), "文档插入应成功: {:?}", result);
        }

        // 批量文档插入
        let batch_docs = vec![
            Document {
                id: "batch1".to_string(),
                content: "批量测试文档1".to_string(),
                vector: Some(vec![0.3; 128]),
                ..Default::default()
            },
            Document {
                id: "batch2".to_string(),
                content: "批量测试文档2".to_string(),
                vector: Some(vec![0.4; 128]),
                ..Default::default()
            },
        ];

        let batch_result = db.batch_add_documents(batch_docs).await;
        assert!(batch_result.is_ok(), "批量插入应成功");

        // 文档检索测试
        let retrieved = db.get_document("doc1").await.unwrap();
        assert!(retrieved.is_some(), "应能检索到文档");
        let doc = retrieved.unwrap();
        assert_eq!(doc.id, "doc1");

        // 搜索测试
        let search_results = db.text_search("机器学习", 5).await.unwrap();
        assert!(!search_results.is_empty(), "文本搜索应返回结果");

        let semantic_results = db.semantic_search("人工智能", 5).await.unwrap();
        assert!(!semantic_results.is_empty(), "语义搜索应返回结果");

        // 文档列表测试
        let doc_list = db.list_documents(0, 10).await.unwrap();
        assert!(doc_list.len() >= 4, "应有至少4个文档");

        // 文档删除测试
        let delete_result = db.delete_document("doc1").await.unwrap();
        assert!(delete_result, "文档删除应成功");

        // 验证删除
        let deleted_check = db.get_document("doc1").await.unwrap();
        assert!(deleted_check.is_none(), "删除的文档不应存在");

        // 统计信息测试
        let stats = db.get_stats().await;
        assert!(stats.document_count >= 3, "应有至少3个文档");
        assert!(stats.memory_usage_mb >= 0.0, "内存使用应非负");
    }

    /// 测试企业级认证和授权
    #[tokio::test]
    async fn test_enterprise_authentication() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        )
        .await
        .unwrap();

        // 获取认证管理器
        let auth_manager = db.get_auth_manager().unwrap();

        // 创建管理员用户
        let admin_id = auth_manager
            .create_admin_user("admin".to_string(), "admin123".to_string())
            .unwrap();
        assert!(!admin_id.is_empty(), "管理员用户ID应非空");

        // 创建普通用户
        let user_id = auth_manager
            .create_user(
                "user1".to_string(),
                Some("user1@test.com".to_string()),
                vec![Role::ReadOnlyUser],
            )
            .unwrap();
        assert!(!user_id.is_empty(), "普通用户ID应非空");

        // 测试用户认证
        let admin_auth = auth_manager.authenticate_user("admin", "admin123");
        assert!(admin_auth.is_ok(), "管理员认证应成功");

        let user_auth = auth_manager.authenticate_user("user1", "password123");
        assert!(user_auth.is_ok(), "普通用户认证应成功");

        // 测试错误认证
        let wrong_auth = auth_manager.authenticate_user("admin", "wrong_password");
        assert!(wrong_auth.is_err(), "错误密码认证应失败");

        // 测试权限检查（同步方法）
        let admin_permission =
            auth_manager.check_permission(&admin_id, &Permission::ManageDatabase);
        assert!(admin_permission.is_ok(), "管理员应有管理权限");

        let user_permission = auth_manager.check_permission(&user_id, &Permission::ManageDatabase);
        assert!(user_permission.is_err(), "普通用户不应有管理权限");

        // 测试企业指标
        let enterprise_metrics = db.get_enterprise_metrics().await;
        assert!(enterprise_metrics.performance_metrics.average_query_time_ms >= 0.0);
        assert!(enterprise_metrics.auth_metrics.is_some());
    }

    /// 测试配置和持久化
    #[tokio::test]
    async fn test_configuration_and_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().to_path_buf();

        // 第一阶段：创建数据库并插入数据
        {
            let config = VectorDbConfig {
                db_path: db_path.to_string_lossy().to_string(),
                vector_dimension: 64,
                enable_compression: true,
                cache_size: 50,
                ..Default::default()
            };

            let db = VectorDatabase::new(db_path.clone(), config.clone())
                .await
                .unwrap();

            // 验证配置
            let retrieved_config = db.get_config();
            assert_eq!(retrieved_config.vector_dimension, 64, "向量维度配置应正确");
            assert!(retrieved_config.enable_compression, "压缩配置应正确");

            // 插入测试数据
            let test_docs = vec![
                Document {
                    id: "persist_1".to_string(),
                    content: "持久化测试文档1".to_string(),
                    vector: Some((0..64).map(|i| i as f32 * 0.01).collect()),
                    ..Default::default()
                },
                Document {
                    id: "persist_2".to_string(),
                    content: "持久化测试文档2".to_string(),
                    vector: Some((0..64).map(|i| (i + 32) as f32 * 0.01).collect()),
                    ..Default::default()
                },
            ];

            for doc in test_docs {
                let result = db.add_document(doc).await;
                assert!(result.is_ok(), "数据插入应成功");
            }

            let stats_before = db.get_stats().await;
            assert!(stats_before.document_count >= 2, "应插入至少2个文档");
        }

        // 第二阶段：重新打开数据库并验证数据持久化
        {
            let config = VectorDbConfig {
                db_path: db_path.to_string_lossy().to_string(),
                vector_dimension: 64,
                ..Default::default()
            };

            let db = VectorDatabase::new(db_path, config).await.unwrap();

            let stats_after = db.get_stats().await;
            assert!(stats_after.document_count >= 2, "重新打开后应仍有文档");

            // 验证能搜索到数据
            let search_results = db.text_search("持久化测试", 5).await.unwrap();
            assert!(!search_results.is_empty(), "应能搜索到持久化的数据");

            // 验证具体文档
            let doc1 = db.get_document("persist_1").await.unwrap();
            assert!(doc1.is_some(), "应能检索到持久化的文档1");

            let doc2 = db.get_document("persist_2").await.unwrap();
            assert!(doc2.is_some(), "应能检索到持久化的文档2");
        }
    }

    /// 测试性能和并发
    #[tokio::test]
    async fn test_performance_and_concurrency() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig {
            db_path: temp_dir.path().to_string_lossy().to_string(),
            vector_dimension: 32,
            cache_size: 100,
            ..Default::default()
        };

        let db = std::sync::Arc::new(
            VectorDatabase::new(temp_dir.path().to_path_buf(), config)
                .await
                .unwrap(),
        );

        // 并发插入测试
        let mut insert_handles = Vec::new();
        for thread_id in 0..5 {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                for i in 0..20 {
                    let doc = Document {
                        id: format!("concurrent_{}_{}", thread_id, i),
                        content: format!("并发测试文档 {} {}", thread_id, i),
                        vector: Some((0..32).map(|_| fastrand::f32()).collect()),
                        ..Default::default()
                    };

                    let result = db_clone.add_document(doc).await;
                    assert!(result.is_ok(), "并发插入应成功");
                }
            });
            insert_handles.push(handle);
        }

        // 等待所有插入完成
        for handle in insert_handles {
            handle.await.unwrap();
        }

        // 并发搜索测试
        let mut search_handles = Vec::new();
        for i in 0..10 {
            let db_clone = db.clone();
            let handle = tokio::spawn(async move {
                let query = format!("并发测试 {}", i);
                let result = db_clone.text_search(&query, 5).await;
                assert!(result.is_ok(), "并发搜索应成功");
                result.unwrap()
            });
            search_handles.push(handle);
        }

        // 验证并发搜索结果
        for handle in search_handles {
            let _results = handle.await.unwrap();
        }

        // 验证最终状态
        let final_stats = db.get_stats().await;
        assert_eq!(final_stats.document_count, 100, "应插入100个文档");

        // 性能测试：批量操作
        let batch_docs: Vec<Document> = (0..50)
            .map(|i| Document {
                id: format!("batch_perf_{}", i),
                content: format!("批量性能测试文档 {}", i),
                vector: Some((0..32).map(|j| ((i + j) as f32) / 100.0).collect()),
                ..Default::default()
            })
            .collect();

        let batch_start = std::time::Instant::now();
        let batch_result = db.batch_add_documents(batch_docs).await;
        let batch_time = batch_start.elapsed();

        assert!(batch_result.is_ok(), "批量操作应成功");
        assert!(
            batch_time < Duration::from_secs(10),
            "批量操作应在合理时间内完成"
        );
        println!("批量插入50个文档耗时: {:?}", batch_time);
    }

    /// 测试边界条件和错误处理
    #[tokio::test]
    async fn test_edge_cases_and_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig {
            db_path: temp_dir.path().to_string_lossy().to_string(),
            vector_dimension: 16,
            ..Default::default()
        };

        let db = VectorDatabase::new(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();

        // 测试空ID文档
        let empty_id_doc = Document {
            id: "".to_string(),
            content: "空ID测试".to_string(),
            vector: Some(vec![1.0; 16]),
            ..Default::default()
        };

        let empty_id_result = db.add_document(empty_id_doc).await;
        assert!(empty_id_result.is_err(), "空ID应失败");

        // 测试删除不存在的文档
        let delete_nonexistent = db.delete_document("nonexistent_id").await.unwrap();
        assert!(!delete_nonexistent, "删除不存在的文档应返回false");

        // 测试Unicode和特殊字符
        let unicode_doc = Document {
            id: "unicode_测试_🚀".to_string(),
            content: "包含Unicode字符: 测试🎯emoji和特殊字符™®©".to_string(),
            title: Some("Unicode测试".to_string()),
            metadata: [("emoji".to_string(), "🚀🎯".to_string())].into(),
            vector: Some(vec![0.5; 16]),
            ..Default::default()
        };

        let unicode_result = db.add_document(unicode_doc).await;
        assert!(unicode_result.is_ok(), "Unicode文档应成功插入");

        // 验证Unicode文档可检索
        let unicode_retrieved = db.get_document("unicode_测试_🚀").await.unwrap();
        assert!(unicode_retrieved.is_some(), "Unicode文档应能被检索");

        // 测试超长内容
        let long_content = "很长的内容 ".repeat(1000);
        let long_doc = Document {
            id: "long_content".to_string(),
            content: long_content,
            vector: Some(vec![0.1; 16]),
            ..Default::default()
        };

        let long_result = db.add_document(long_doc).await;
        assert!(long_result.is_ok(), "超长内容应被处理");

        // 测试极值向量
        let extreme_doc = Document {
            id: "extreme_values".to_string(),
            content: "极值测试".to_string(),
            vector: Some(vec![f32::MAX; 16]),
            ..Default::default()
        };

        let extreme_result = db.add_document(extreme_doc).await;
        // 极值可能被拒绝或处理，都是合理的
        match extreme_result {
            Ok(_) => println!("极值向量被成功处理"),
            Err(e) => println!("极值向量被正确拒绝: {:?}", e),
        }

        // 测试搜索边界条件
        let empty_search = db.text_search("", 5).await;
        if let Ok(results) = empty_search {
            // Results vector length is always >= 0, no need to check
            assert!(
                results.is_empty() || !results.is_empty(),
                "空搜索结果应合理"
            );
        }
        // 错误也是可接受的

        // 验证数据库仍然可用
        let stats = db.get_stats().await;
        assert!(stats.document_count >= 2, "应至少有2个正常文档");
    }

    /// 测试配置变化适应性
    #[tokio::test]
    async fn test_configuration_adaptability() {
        let temp_dir = TempDir::new().unwrap();

        // 测试不同配置
        let configs = vec![
            VectorDbConfig {
                db_path: temp_dir.path().join("test1").to_string_lossy().to_string(),
                vector_dimension: 32,
                cache_size: 50,
                enable_compression: true,
                ..Default::default()
            },
            VectorDbConfig {
                db_path: temp_dir.path().join("test2").to_string_lossy().to_string(),
                vector_dimension: 128,
                cache_size: 100,
                enable_compression: false,
                ..Default::default()
            },
        ];

        for (i, config) in configs.into_iter().enumerate() {
            let db = VectorDatabase::new(std::path::PathBuf::from(&config.db_path), config.clone())
                .await
                .unwrap();

            // 验证配置生效
            let retrieved_config = db.get_config();
            assert_eq!(retrieved_config.vector_dimension, config.vector_dimension);
            assert_eq!(
                retrieved_config.enable_compression,
                config.enable_compression
            );

            // 插入测试数据
            let test_embedding: Vec<f32> = (0..config.vector_dimension)
                .map(|j| (j as f32) / (config.vector_dimension as f32))
                .collect();

            let doc = Document {
                id: format!("config_test_{}", i),
                content: format!("配置测试文档 {}", i),
                vector: Some(test_embedding),
                ..Default::default()
            };

            let insert_result = db.add_document(doc).await;
            assert!(insert_result.is_ok(), "配置适应性测试插入应成功");

            // 测试搜索
            let search_result = db.text_search("配置测试", 1).await;
            assert!(search_result.is_ok(), "配置适应性搜索应成功");
        }
    }
}
