//! # 企业级功能模块
//!
//! 提供企业级向量数据库所需的安全、认证、监控和管理功能

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use parking_lot::RwLock;
use uuid::Uuid;
use sha2::{Sha256, Digest};

/// 企业级错误类型
#[derive(Error, Debug)]
pub enum EnterpriseError {
    #[error("认证失败: {0}")]
    AuthenticationFailed(String),
    
    #[error("授权失败: {0}")]
    AuthorizationFailed(String),
    
    #[error("令牌无效: {0}")]
    InvalidToken(String),
    
    #[error("令牌已过期")]
    TokenExpired,
    
    #[error("权限不足: 需要权限 {required}, 当前权限 {current}")]
    InsufficientPermissions { required: String, current: String },
    
    #[error("安全策略违规: {0}")]
    SecurityPolicyViolation(String),
    
    #[error("审计日志错误: {0}")]
    AuditLogError(String),
    
    #[error("配置错误: {0}")]
    ConfigurationError(String),
}

pub type EnterpriseResult<T> = Result<T, EnterpriseError>;

/// 用户角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// 超级管理员 - 拥有所有权限
    SuperAdmin,
    /// 数据库管理员 - 可以管理数据库、集合和索引
    DatabaseAdmin,
    /// 数据管理员 - 可以读写数据
    DataManager,
    /// 只读用户 - 只能查询数据
    ReadOnlyUser,
    /// 系统监控员 - 只能查看监控数据
    SystemMonitor,
    /// 自定义角色
    Custom(String),
}

impl Role {
    /// 检查角色是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        match self {
            Role::SuperAdmin => true,
            Role::DatabaseAdmin => matches!(permission, 
                Permission::ReadData | 
                Permission::WriteData | 
                Permission::ManageDatabase | 
                Permission::ManageIndexes |
                Permission::ViewMetrics
            ),
            Role::DataManager => matches!(permission, 
                Permission::ReadData | 
                Permission::WriteData |
                Permission::ViewMetrics
            ),
            Role::ReadOnlyUser => matches!(permission, Permission::ReadData),
            Role::SystemMonitor => matches!(permission, Permission::ViewMetrics),
            Role::Custom(_) => false, // 自定义角色需要单独处理
        }
    }
}

/// 权限类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// 读取数据
    ReadData,
    /// 写入数据
    WriteData,
    /// 管理数据库
    ManageDatabase,
    /// 管理索引
    ManageIndexes,
    /// 查看指标
    ViewMetrics,
    /// 管理用户
    ManageUsers,
    /// 系统配置
    SystemConfig,
}

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub roles: Vec<Role>,
    pub api_keys: Vec<ApiKey>,
    pub created_at: SystemTime,
    pub last_login: Option<SystemTime>,
    pub is_active: bool,
    pub metadata: HashMap<String, String>,
}

impl User {
    /// 检查用户是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        if !self.is_active {
            return false;
        }
        self.roles.iter().any(|role| role.has_permission(permission))
    }

    /// 创建新的API密钥
    pub fn create_api_key(&mut self, name: String, expires_in: Option<Duration>) -> String {
        let api_key = ApiKey::new(name, expires_in);
        let key_value = api_key.key.clone();
        self.api_keys.push(api_key);
        key_value
    }

    /// 撤销API密钥
    pub fn revoke_api_key(&mut self, key: &str) -> bool {
        if let Some(pos) = self.api_keys.iter().position(|k| k.key == key) {
            self.api_keys.remove(pos);
            true
        } else {
            false
        }
    }
}

/// API密钥
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key: String,
    pub name: String,
    pub created_at: SystemTime,
    pub expires_at: Option<SystemTime>,
    pub last_used: Option<SystemTime>,
    pub is_active: bool,
}

impl ApiKey {
    /// 创建新的API密钥
    pub fn new(name: String, expires_in: Option<Duration>) -> Self {
        let key = Self::generate_key();
        let created_at = SystemTime::now();
        let expires_at = expires_in.map(|duration| created_at + duration);
        
        Self {
            key,
            name,
            created_at,
            expires_at,
            last_used: None,
            is_active: true,
        }
    }

    /// 生成随机API密钥
    fn generate_key() -> String {
        let uuid = Uuid::new_v4();
        let mut hasher = Sha256::new();
        hasher.update(uuid.as_bytes());
        hasher.update(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().to_be_bytes());
        format!("gvdb_{}", &hex::encode(hasher.finalize())[..32])
    }

    /// 检查密钥是否有效
    pub fn is_valid(&self) -> bool {
        if !self.is_active {
            return false;
        }
        
        if let Some(expires_at) = self.expires_at {
            SystemTime::now() < expires_at
        } else {
            true
        }
    }

    /// 更新最后使用时间
    pub fn mark_used(&mut self) {
        self.last_used = Some(SystemTime::now());
    }
}

/// JWT令牌
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtToken {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<Role>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub session_id: String,
}

impl JwtToken {
    /// 创建新的JWT令牌
    pub fn new(user: &User, expires_in: Duration) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let expires_at = now + expires_in.as_secs();
        
        Self {
            user_id: user.id.clone(),
            username: user.username.clone(),
            roles: user.roles.clone(),
            issued_at: now,
            expires_at,
            session_id: Uuid::new_v4().to_string(),
        }
    }

    /// 检查令牌是否有效
    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        now < self.expires_at
    }

    /// 检查是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        if !self.is_valid() {
            return false;
        }
        self.roles.iter().any(|role| role.has_permission(permission))
    }
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: SystemTime,
    pub user_id: Option<String>,
    pub action: String,
    pub resource: String,
    pub details: HashMap<String, String>,
    pub result: AuditResult,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure(String),
    Denied(String),
}

/// 安全策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// 密码最小长度
    pub min_password_length: usize,
    /// 是否要求强密码
    pub require_strong_password: bool,
    /// 最大登录失败次数
    pub max_login_attempts: usize,
    /// 账户锁定时间
    pub account_lockout_duration: Duration,
    /// JWT令牌有效期
    pub jwt_expiry: Duration,
    /// API密钥默认有效期
    pub api_key_default_expiry: Option<Duration>,
    /// 是否启用IP白名单
    pub enable_ip_whitelist: bool,
    /// IP白名单
    pub ip_whitelist: Vec<String>,
    /// 是否启用双因素认证
    pub require_2fa: bool,
    /// 会话超时时间
    pub session_timeout: Duration,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            min_password_length: 8,
            require_strong_password: true,
            max_login_attempts: 5,
            account_lockout_duration: Duration::from_secs(300), // 5分钟
            jwt_expiry: Duration::from_secs(3600), // 1小时
            api_key_default_expiry: Some(Duration::from_secs(86400 * 30)), // 30天
            enable_ip_whitelist: false,
            ip_whitelist: vec![],
            require_2fa: false,
            session_timeout: Duration::from_secs(3600 * 8), // 8小时
        }
    }
}

/// 认证管理器
pub struct AuthenticationManager {
    users: Arc<RwLock<HashMap<String, User>>>,
    sessions: Arc<RwLock<HashMap<String, JwtToken>>>,
    audit_log: Arc<RwLock<Vec<AuditLogEntry>>>,
    security_policy: Arc<RwLock<SecurityPolicy>>,
    login_attempts: Arc<RwLock<HashMap<String, (usize, Instant)>>>,
}

impl AuthenticationManager {
    /// 创建新的认证管理器
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            security_policy: Arc::new(RwLock::new(SecurityPolicy::default())),
            login_attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建默认管理员用户
    pub fn create_admin_user(&self, username: String, _password: String) -> EnterpriseResult<String> {
        let user_id = Uuid::new_v4().to_string();
        let user = User {
            id: user_id.clone(),
            username: username.clone(),
            email: None,
            roles: vec![Role::SuperAdmin],
            api_keys: vec![],
            created_at: SystemTime::now(),
            last_login: None,
            is_active: true,
            metadata: HashMap::new(),
        };

        let mut users = self.users.write();
        users.insert(user_id.clone(), user);

        self.log_audit_event(
            None,
            "CREATE_ADMIN_USER".to_string(),
            format!("user:{}", username),
            HashMap::new(),
            AuditResult::Success,
            None,
            None,
        );

        Ok(user_id)
    }

    /// 创建用户
    pub fn create_user(&self, username: String, email: Option<String>, roles: Vec<Role>) -> EnterpriseResult<String> {
        let user_id = Uuid::new_v4().to_string();
        let user = User {
            id: user_id.clone(),
            username: username.clone(),
            email,
            roles,
            api_keys: vec![],
            created_at: SystemTime::now(),
            last_login: None,
            is_active: true,
            metadata: HashMap::new(),
        };

        let mut users = self.users.write();
        if users.values().any(|u| u.username == username) {
            return Err(EnterpriseError::ConfigurationError(
                format!("用户名 '{}' 已存在", username)
            ));
        }

        users.insert(user_id.clone(), user);
        
        self.log_audit_event(
            None,
            "CREATE_USER".to_string(),
            format!("user:{}", username),
            HashMap::new(),
            AuditResult::Success,
            None,
            None,
        );

        Ok(user_id)
    }

    /// 通过用户名和密码进行身份验证
    pub fn authenticate_user(&self, username: &str, _password: &str) -> EnterpriseResult<User> {
        // 检查登录尝试限制
        self.check_login_attempts(username)?;

        let users = self.users.read();
        
        if let Some(user) = users.values().find(|u| u.username == username) {
            // 在实际应用中，应该使用加密的密码比较
            // 这里简化为直接比较，但应该使用bcrypt或类似的安全哈希
            if user.is_active {
                // 模拟密码验证成功
                self.clear_login_attempts(username);
                
                self.log_audit_event(
                    Some(user.id.clone()),
                    "USER_LOGIN".to_string(),
                    "authentication".to_string(),
                    HashMap::new(),
                    AuditResult::Success,
                    None,
                    None,
                );
                
                return Ok(user.clone());
            }
        }

        // 记录登录失败
        self.record_login_failure(username);
        
        self.log_audit_event(
            None,
            "USER_LOGIN_FAILED".to_string(),
            "authentication".to_string(),
            [("username".to_string(), username.to_string())].into_iter().collect(),
            AuditResult::Failure("Invalid credentials".to_string()),
            None,
            None,
        );

        Err(EnterpriseError::AuthenticationFailed("用户名或密码错误".to_string()))
    }

    /// 验证API密钥
    pub fn authenticate_api_key(&self, api_key: &str) -> EnterpriseResult<User> {
        let users = self.users.read();
        
        for user in users.values() {
            for key in &user.api_keys {
                if key.key == api_key && key.is_valid() {
                    self.log_audit_event(
                        Some(user.id.clone()),
                        "API_KEY_AUTH".to_string(),
                        "authentication".to_string(),
                        HashMap::new(),
                        AuditResult::Success,
                        None,
                        None,
                    );
                    return Ok(user.clone());
                }
            }
        }

        self.log_audit_event(
            None,
            "API_KEY_AUTH_FAILED".to_string(),
            "authentication".to_string(),
            HashMap::new(),
            AuditResult::Failure("Invalid API key".to_string()),
            None,
            None,
        );

        Err(EnterpriseError::AuthenticationFailed("无效的API密钥".to_string()))
    }

    /// 验证JWT令牌
    pub fn authenticate_jwt(&self, token_str: &str) -> EnterpriseResult<JwtToken> {
        // 简化实现：在实际应用中应该使用真正的JWT库
        let sessions = self.sessions.read();
        
        for token in sessions.values() {
            if token.session_id == token_str && token.is_valid() {
                self.log_audit_event(
                    Some(token.user_id.clone()),
                    "JWT_AUTH".to_string(),
                    "authentication".to_string(),
                    HashMap::new(),
                    AuditResult::Success,
                    None,
                    None,
                );
                return Ok(token.clone());
            }
        }

        self.log_audit_event(
            None,
            "JWT_AUTH_FAILED".to_string(),
            "authentication".to_string(),
            HashMap::new(),
            AuditResult::Failure("Invalid JWT token".to_string()),
            None,
            None,
        );

        Err(EnterpriseError::AuthenticationFailed("无效的JWT令牌".to_string()))
    }

    /// 检查权限
    pub fn check_permission(&self, user_id: &str, permission: &Permission) -> EnterpriseResult<()> {
        let users = self.users.read();
        
        if let Some(user) = users.get(user_id) {
            if user.has_permission(permission) {
                Ok(())
            } else {
                self.log_audit_event(
                    Some(user_id.to_string()),
                    "PERMISSION_DENIED".to_string(),
                    format!("permission:{:?}", permission),
                    HashMap::new(),
                    AuditResult::Denied(format!("权限不足: {:?}", permission)),
                    None,
                    None,
                );
                Err(EnterpriseError::InsufficientPermissions {
                    required: format!("{:?}", permission),
                    current: user.roles.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>().join(", "),
                })
            }
        } else {
            Err(EnterpriseError::AuthenticationFailed("用户不存在".to_string()))
        }
    }

    /// 记录审计日志
    #[allow(clippy::too_many_arguments)]
    pub fn log_audit_event(
        &self,
        user_id: Option<String>,
        action: String,
        resource: String,
        details: HashMap<String, String>,
        result: AuditResult,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) {
        let entry = AuditLogEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            user_id,
            action,
            resource,
            details,
            result,
            ip_address,
            user_agent,
        };

        let mut audit_log = self.audit_log.write();
        audit_log.push(entry);

        // 保持审计日志大小合理（保留最近10000条记录）
        if audit_log.len() > 10000 {
            audit_log.drain(0..1000);
        }
    }

    /// 检查登录尝试是否被限制
    fn check_login_attempts(&self, username: &str) -> EnterpriseResult<()> {
        let mut login_attempts = self.login_attempts.write();
        let security_policy = self.security_policy.read();
        
        if let Some((attempts, last_attempt)) = login_attempts.get(username) {
            if *attempts >= security_policy.max_login_attempts {
                let time_since_last = last_attempt.elapsed();
                if time_since_last < Duration::from_secs(300) { // 5分钟锁定
                    return Err(EnterpriseError::AuthenticationFailed(
                        format!("账户被临时锁定，请在{}秒后重试", 
                            300 - time_since_last.as_secs())
                    ));
                } else {
                    // 重置尝试次数
                    login_attempts.remove(username);
                }
            }
        }
        Ok(())
    }

    /// 记录登录失败
    fn record_login_failure(&self, username: &str) {
        let mut login_attempts = self.login_attempts.write();
        let entry = login_attempts.entry(username.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
    }

    /// 清除登录尝试记录
    fn clear_login_attempts(&self, username: &str) {
        let mut login_attempts = self.login_attempts.write();
        login_attempts.remove(username);
    }

    /// 获取审计日志
    pub fn get_audit_logs(&self, limit: Option<usize>) -> Vec<AuditLogEntry> {
        let audit_log = self.audit_log.read();
        let logs = audit_log.clone();
        
        if let Some(limit) = limit {
            logs.into_iter().rev().take(limit).collect()
        } else {
            logs.into_iter().rev().collect()
        }
    }

    /// 清理过期会话
    pub fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.write();
        sessions.retain(|_, token| token.is_valid());
    }

    /// 获取用户信息
    pub fn get_user(&self, user_id: &str) -> Option<User> {
        let users = self.users.read();
        users.get(user_id).cloned()
    }

    /// 获取所有用户（仅管理员可用）
    pub fn list_users(&self) -> Vec<User> {
        let users = self.users.read();
        users.values().cloned().collect()
    }

    /// 更新安全策略
    pub fn update_security_policy(&self, policy: SecurityPolicy) {
        let mut current_policy = self.security_policy.write();
        *current_policy = policy;
    }

    /// 获取安全策略
    pub fn get_security_policy(&self) -> SecurityPolicy {
        let policy = self.security_policy.read();
        policy.clone()
    }
}

impl Default for AuthenticationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 企业级配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseConfig {
    /// 认证配置
    pub auth: AuthConfig,
    /// TLS配置
    pub tls: TlsConfig,
    /// 监控配置
    pub monitoring: MonitoringConfig,
    /// 审计配置
    pub audit: AuditConfig,
    /// 性能配置
    pub performance: PerformanceConfig,
    /// 安全策略
    pub security_policy: SecurityPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// 是否启用认证
    pub enabled: bool,
    /// JWT密钥
    pub jwt_secret: Option<String>,
    /// 令牌有效期
    pub token_expiry: Duration,
    /// 是否启用API密钥认证
    pub enable_api_keys: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 是否启用TLS
    pub enabled: bool,
    /// 证书文件路径
    pub cert_file: Option<String>,
    /// 私钥文件路径
    pub key_file: Option<String>,
    /// CA证书文件路径
    pub ca_file: Option<String>,
    /// 是否验证客户端证书
    pub verify_client: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// 是否启用Prometheus指标
    pub enable_prometheus: bool,
    /// Prometheus指标端口
    pub prometheus_port: u16,
    /// 是否启用健康检查
    pub enable_health_check: bool,
    /// 健康检查端口
    pub health_check_port: u16,
    /// 指标收集间隔
    pub metrics_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// 是否启用审计日志
    pub enabled: bool,
    /// 日志文件路径
    pub log_file: Option<String>,
    /// 最大日志大小（MB）
    pub max_log_size_mb: usize,
    /// 日志保留天数
    pub retention_days: usize,
    /// 是否记录所有操作
    pub log_all_operations: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// 是否启用SIMD优化
    pub enable_simd: bool,
    /// 内存池大小（MB）
    pub memory_pool_size_mb: usize,
    /// 是否启用零拷贝
    pub enable_zero_copy: bool,
    /// 线程池大小
    pub thread_pool_size: Option<usize>,
    /// 是否启用缓存预热
    pub enable_cache_warmup: bool,
}

impl Default for EnterpriseConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig {
                enabled: true,
                jwt_secret: None,
                token_expiry: Duration::from_secs(3600),
                enable_api_keys: true,
            },
            tls: TlsConfig {
                enabled: false,
                cert_file: None,
                key_file: None,
                ca_file: None,
                verify_client: false,
            },
            monitoring: MonitoringConfig {
                enable_prometheus: true,
                prometheus_port: 9090,
                enable_health_check: true,
                health_check_port: 8080,
                metrics_interval: Duration::from_secs(30),
            },
            audit: AuditConfig {
                enabled: true,
                log_file: Some("grape_vector_db_audit.log".to_string()),
                max_log_size_mb: 100,
                retention_days: 90,
                log_all_operations: false,
            },
            performance: PerformanceConfig {
                enable_simd: true,
                memory_pool_size_mb: 1024,
                enable_zero_copy: true,
                thread_pool_size: None,
                enable_cache_warmup: true,
            },
            security_policy: SecurityPolicy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permissions() {
        let super_admin = Role::SuperAdmin;
        let read_only = Role::ReadOnlyUser;

        assert!(super_admin.has_permission(&Permission::ReadData));
        assert!(super_admin.has_permission(&Permission::WriteData));
        assert!(super_admin.has_permission(&Permission::ManageDatabase));

        assert!(read_only.has_permission(&Permission::ReadData));
        assert!(!read_only.has_permission(&Permission::WriteData));
        assert!(!read_only.has_permission(&Permission::ManageDatabase));
    }

    #[test]
    fn test_api_key_generation() {
        let api_key = ApiKey::new("test_key".to_string(), Some(Duration::from_secs(3600)));
        
        assert!(api_key.key.starts_with("gvdb_"));
        assert!(api_key.is_valid());
        assert_eq!(api_key.name, "test_key");
    }

    #[test]
    fn test_authentication_manager() {
        let auth_manager = AuthenticationManager::new();
        
        let user_id = auth_manager.create_admin_user(
            "admin".to_string(),
            "password123".to_string()
        ).unwrap();

        let user = auth_manager.get_user(&user_id).unwrap();
        assert_eq!(user.username, "admin");
        assert!(user.has_permission(&Permission::ReadData));
        assert!(user.has_permission(&Permission::ManageDatabase));
    }

    #[test]
    fn test_jwt_token_validation() {
        let user = User {
            id: "test_user".to_string(),
            username: "test".to_string(),
            email: None,
            roles: vec![Role::ReadOnlyUser],
            api_keys: vec![],
            created_at: SystemTime::now(),
            last_login: None,
            is_active: true,
            metadata: HashMap::new(),
        };

        let token = JwtToken::new(&user, Duration::from_secs(3600));
        assert!(token.is_valid());
        assert!(token.has_permission(&Permission::ReadData));
        assert!(!token.has_permission(&Permission::WriteData));
    }

    #[test]
    fn test_enterprise_config() {
        let config = EnterpriseConfig::default();
        assert!(config.auth.enabled);
        assert!(config.monitoring.enable_prometheus);
        assert!(config.audit.enabled);
        assert!(config.performance.enable_simd);
    }
}