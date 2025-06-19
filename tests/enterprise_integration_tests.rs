//! 企业级功能集成测试

use grape_vector_db::*;
use std::time::Duration;
use tempfile::TempDir;

#[cfg(test)]
mod enterprise_tests {
    use super::*;

    #[tokio::test]
    async fn test_enterprise_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        ).await;

        assert!(db.is_ok(), "企业版数据库创建应该成功");
        
        let db = db.unwrap();
        assert!(db.get_auth_manager().is_some(), "应该包含认证管理器");
        assert!(db.get_resilience_manager().is_some(), "应该包含韧性管理器");
        assert!(db.get_enterprise_config().is_some(), "应该包含企业配置");
    }

    #[tokio::test]
    async fn test_authentication_and_authorization() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        ).await.unwrap();

        // 获取认证管理器
        let auth_manager = db.get_auth_manager().unwrap();

        // 创建管理员用户
        let admin_user_id = auth_manager.create_admin_user(
            "admin".to_string(),
            "password123".to_string(),
        ).unwrap();

        // 创建普通用户
        let user_id = auth_manager.create_user(
            "user1".to_string(),
            Some("user1@example.com".to_string()),
            vec![Role::ReadOnlyUser],
        ).unwrap();

        // 为管理员用户创建API密钥
        let admin_user = auth_manager.get_user(&admin_user_id).unwrap();
        let mut admin_user_mut = admin_user.clone();
        let admin_api_key = admin_user_mut.create_api_key(
            "admin_key".to_string(),
            Some(Duration::from_secs(3600)),
        );

        // 为普通用户创建API密钥
        let user = auth_manager.get_user(&user_id).unwrap();
        let mut user_mut = user.clone();
        let user_api_key = user_mut.create_api_key(
            "user_key".to_string(),
            Some(Duration::from_secs(3600)),
        );

        // 测试文档操作权限
        let test_doc = Document {
            id: "test_doc_1".to_string(),
            title: Some("测试文档".to_string()),
            content: "这是一个测试文档".to_string(),
            language: Some("zh".to_string()),
            version: Some("1".to_string()),
            doc_type: Some("test".to_string()),
            package_name: Some("test_package".to_string()),
            vector: Some(vec![0.1, 0.2, 0.3, 0.4]),
            metadata: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // 管理员应该能够添加文档
        let result = db.add_document_enterprise(
            test_doc.clone(),
            Some(admin_api_key.clone()),
        ).await;
        assert!(result.is_ok(), "管理员应该能够添加文档");

        // 普通用户应该能够搜索文档
        let search_result = db.search_enterprise(
            "测试",
            10,
            Some(user_api_key.clone()),
        ).await;
        assert!(search_result.is_ok(), "普通用户应该能够搜索文档");

        // 普通用户应该无法添加文档（权限不足）
        let user_add_result = db.add_document_enterprise(
            test_doc.clone(),
            Some(user_api_key.clone()),
        ).await;
        assert!(user_add_result.is_err(), "普通用户不应该能够添加文档");
    }

    #[tokio::test]
    async fn test_resilience_features() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        ).await.unwrap();

        // 获取韧性管理器
        let resilience_manager = db.get_resilience_manager().unwrap();

        // 测试熔断器
        let circuit_breaker = resilience_manager.get_circuit_breaker("vector_search");
        assert!(circuit_breaker.is_some(), "应该有向量搜索的熔断器");

        let cb = circuit_breaker.unwrap();
        assert_eq!(cb.get_state(), CircuitBreakerState::Closed, "初始状态应该是关闭的");

        // 测试限流器
        let rate_limiter = resilience_manager.get_rate_limiter("api_requests");
        assert!(rate_limiter.is_some(), "应该有API请求的限流器");

        let rl = rate_limiter.unwrap();
        assert!(rl.try_acquire(1), "应该能够获取令牌");

        // 测试韧性状态
        let resilience_status = resilience_manager.get_resilience_status();
        assert!(resilience_status.circuit_breakers.contains_key("vector_search"));
        assert!(resilience_status.rate_limiters.contains_key("api_requests"));
    }

    #[tokio::test]
    async fn test_health_monitoring() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        ).await.unwrap();

        // 测试健康状态检查
        let health_status = db.get_health_status().await;
        assert_eq!(health_status.overall_status, HealthStatus::Healthy);
        assert_eq!(health_status.database_status, HealthStatus::Healthy);
        assert!(health_status.auth_status.is_some());
        assert!(health_status.resilience_status.is_some());

        // 测试企业指标 (省略无意义的非负数检查)
        let enterprise_metrics = db.get_enterprise_metrics().await;
        assert!(enterprise_metrics.performance_metrics.average_query_time_ms >= 0.0);
        assert!(enterprise_metrics.auth_metrics.is_some());
        assert!(enterprise_metrics.resilience_metrics.is_some());
    }

    #[tokio::test]
    async fn test_audit_logging() {
        let temp_dir = TempDir::new().unwrap();
        let config = VectorDbConfig::default();
        let enterprise_config = EnterpriseConfig::default();

        let db = VectorDatabase::new_enterprise(
            temp_dir.path().to_path_buf(),
            config,
            enterprise_config,
        ).await.unwrap();

        let auth_manager = db.get_auth_manager().unwrap();

        // 创建用户（这应该生成审计日志）
        let _user_id = auth_manager.create_user(
            "test_user".to_string(),
            Some("test@example.com".to_string()),
            vec![Role::DataManager],
        ).unwrap();

        // 检查审计日志
        let audit_logs = auth_manager.get_audit_logs(Some(10));
        assert!(!audit_logs.is_empty(), "应该有审计日志记录");

        let create_user_log = audit_logs.iter()
            .find(|log| log.action == "CREATE_USER");
        assert!(create_user_log.is_some(), "应该有创建用户的审计日志");

        let log = create_user_log.unwrap();
        assert!(log.resource.contains("test_user"));
        assert!(matches!(log.result, crate::enterprise::AuditResult::Success));
    }

    #[tokio::test]
    async fn test_security_policies() {
        let auth_manager = AuthenticationManager::new();

        // 测试默认安全策略
        let default_policy = auth_manager.get_security_policy();
        assert_eq!(default_policy.min_password_length, 8);
        assert!(default_policy.require_strong_password);
        assert_eq!(default_policy.max_login_attempts, 5);

        // 更新安全策略
        let mut new_policy = default_policy.clone();
        new_policy.min_password_length = 12;
        new_policy.max_login_attempts = 3;
        auth_manager.update_security_policy(new_policy);

        let updated_policy = auth_manager.get_security_policy();
        assert_eq!(updated_policy.min_password_length, 12);
        assert_eq!(updated_policy.max_login_attempts, 3);
    }

    #[tokio::test]
    async fn test_api_key_management() {
        let auth_manager = AuthenticationManager::new();

        // 创建用户
        let user_id = auth_manager.create_user(
            "api_test_user".to_string(),
            Some("api@example.com".to_string()),
            vec![Role::DataManager],
        ).unwrap();

        // 获取用户并创建API密钥
        let user = auth_manager.get_user(&user_id).unwrap();
        let mut user_mut = user.clone();
        let api_key = user_mut.create_api_key(
            "test_api_key".to_string(),
            Some(Duration::from_secs(3600)),
        );

        assert!(api_key.starts_with("gvdb_"), "API密钥应该有正确的前缀");

        // 测试API密钥验证
        let auth_result = auth_manager.authenticate_api_key(&api_key);
        assert!(auth_result.is_ok(), "API密钥验证应该成功");

        let authenticated_user = auth_result.unwrap();
        assert_eq!(authenticated_user.id, user_id);
        assert!(authenticated_user.has_permission(&Permission::WriteData));

        // 撤销API密钥
        let revoke_result = user_mut.revoke_api_key(&api_key);
        assert!(revoke_result, "API密钥撤销应该成功");

        // 撤销后的验证应该失败
        let auth_result_after_revoke = auth_manager.authenticate_api_key(&api_key);
        assert!(auth_result_after_revoke.is_err(), "撤销后的API密钥验证应该失败");
    }

    #[tokio::test]
    async fn test_role_based_permissions() {
        // 测试不同角色的权限
        let super_admin = Role::SuperAdmin;
        let db_admin = Role::DatabaseAdmin;
        let data_manager = Role::DataManager;
        let read_only = Role::ReadOnlyUser;
        let monitor = Role::SystemMonitor;

        // SuperAdmin应该有所有权限
        assert!(super_admin.has_permission(&Permission::ReadData));
        assert!(super_admin.has_permission(&Permission::WriteData));
        assert!(super_admin.has_permission(&Permission::ManageDatabase));
        assert!(super_admin.has_permission(&Permission::ViewMetrics));
        assert!(super_admin.has_permission(&Permission::ManageUsers));

        // DatabaseAdmin应该有数据库管理权限
        assert!(db_admin.has_permission(&Permission::ReadData));
        assert!(db_admin.has_permission(&Permission::WriteData));
        assert!(db_admin.has_permission(&Permission::ManageDatabase));
        assert!(db_admin.has_permission(&Permission::ViewMetrics));
        assert!(!db_admin.has_permission(&Permission::ManageUsers));

        // DataManager应该有数据读写权限
        assert!(data_manager.has_permission(&Permission::ReadData));
        assert!(data_manager.has_permission(&Permission::WriteData));
        assert!(!data_manager.has_permission(&Permission::ManageDatabase));

        // ReadOnlyUser只能读取数据
        assert!(read_only.has_permission(&Permission::ReadData));
        assert!(!read_only.has_permission(&Permission::WriteData));
        assert!(!read_only.has_permission(&Permission::ManageDatabase));

        // SystemMonitor只能查看指标
        assert!(monitor.has_permission(&Permission::ViewMetrics));
        assert!(!monitor.has_permission(&Permission::ReadData));
        assert!(!monitor.has_permission(&Permission::WriteData));
    }

    #[tokio::test]
    async fn test_enterprise_configuration() {
        // 测试企业配置的默认值
        let config = EnterpriseConfig::default();

        assert!(config.auth.enabled);
        assert!(config.auth.enable_api_keys);
        assert_eq!(config.auth.token_expiry, Duration::from_secs(3600));

        assert!(config.monitoring.enable_prometheus);
        assert!(config.monitoring.enable_health_check);
        assert_eq!(config.monitoring.prometheus_port, 9090);
        assert_eq!(config.monitoring.health_check_port, 8080);

        assert!(config.audit.enabled);
        assert_eq!(config.audit.max_log_size_mb, 100);
        assert_eq!(config.audit.retention_days, 90);

        assert!(config.performance.enable_simd);
        assert!(config.performance.enable_zero_copy);
        assert_eq!(config.performance.memory_pool_size_mb, 1024);

        assert_eq!(config.security_policy.min_password_length, 8);
        assert!(config.security_policy.require_strong_password);
        assert_eq!(config.security_policy.max_login_attempts, 5);
    }
}