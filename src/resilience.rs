//! # 企业级错误处理和韧性模块
//!
//! 提供企业级应用所需的错误处理、重试机制、熔断器、限流和监控功能

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use parking_lot::{RwLock, Mutex};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// 企业级韧性错误类型
#[derive(Error, Debug)]
pub enum ResilienceError {
    #[error("熔断器已打开: {service}")]
    CircuitBreakerOpen { service: String },
    
    #[error("限流触发: 当前QPS {current_qps} 超过限制 {limit}")]
    RateLimitExceeded { current_qps: f64, limit: f64 },
    
    #[error("超时: 操作在 {timeout:?} 内未完成")]
    Timeout { timeout: Duration },
    
    #[error("重试次数已达上限: {attempts}")]
    MaxRetriesExceeded { attempts: usize },
    
    #[error("服务不可用: {service}")]
    ServiceUnavailable { service: String },
    
    #[error("资源池耗尽: {resource}")]
    ResourcePoolExhausted { resource: String },
    
    #[error("依赖服务错误: {service} - {error}")]
    DependencyError { service: String, error: String },
}

pub type ResilienceResult<T> = Result<T, ResilienceError>;

/// 熔断器状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// 关闭状态 - 正常工作
    Closed,
    /// 打开状态 - 拒绝请求
    Open,
    /// 半开状态 - 允许少量请求测试
    HalfOpen,
}

/// 熔断器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 失败阈值（失败率百分比）
    pub failure_threshold: f64,
    /// 最小请求数量（只有达到这个数量才会触发熔断）
    pub minimum_requests: usize,
    /// 请求计数窗口时间
    pub request_volume_threshold_period: Duration,
    /// 熔断器打开后的恢复时间
    pub sleep_window: Duration,
    /// 半开状态下允许的请求数量
    pub half_open_max_requests: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 50.0, // 50% 失败率
            minimum_requests: 20,
            request_volume_threshold_period: Duration::from_secs(60),
            sleep_window: Duration::from_secs(30),
            half_open_max_requests: 5,
        }
    }
}

/// 熔断器实现
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
    failure_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    last_request_time: Arc<RwLock<Option<Instant>>>,
    half_open_requests: Arc<AtomicU64>,
    service_name: String,
}

impl CircuitBreaker {
    /// 创建新的熔断器
    pub fn new(service_name: String, config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState::Closed)),
            failure_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            last_failure_time: Arc::new(RwLock::new(None)),
            last_request_time: Arc::new(RwLock::new(None)),
            half_open_requests: Arc::new(AtomicU64::new(0)),
            service_name,
        }
    }

    /// 检查是否允许请求通过
    pub fn allow_request(&self) -> bool {
        *self.last_request_time.write() = Some(Instant::now());
        
        match *self.state.read() {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // 检查是否可以进入半开状态
                if let Some(last_failure) = *self.last_failure_time.read() {
                    if last_failure.elapsed() >= self.config.sleep_window {
                        self.transition_to_half_open();
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => {
                let current_requests = self.half_open_requests.load(Ordering::Relaxed);
                current_requests < self.config.half_open_max_requests as u64
            }
        }
    }

    /// 记录成功的请求
    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::Relaxed);
        
        match *self.state.read() {
            CircuitBreakerState::HalfOpen => {
                // 在半开状态下，如果成功请求足够多，则关闭熔断器
                let success_count = self.success_count.load(Ordering::Relaxed);
                if success_count >= self.config.half_open_max_requests as u64 {
                    self.transition_to_closed();
                }
            }
            _ => {
                // 检查是否需要重置计数器
                self.reset_counters_if_needed();
            }
        }
    }

    /// 记录失败的请求
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        *self.last_failure_time.write() = Some(Instant::now());
        
        match *self.state.read() {
            CircuitBreakerState::HalfOpen => {
                // 在半开状态下失败，立即回到打开状态
                self.transition_to_open();
            }
            CircuitBreakerState::Closed => {
                // 检查是否需要打开熔断器
                if self.should_trip() {
                    self.transition_to_open();
                }
            }
            CircuitBreakerState::Open => {
                // 已经打开，不需要额外操作
            }
        }
    }

    /// 检查是否应该触发熔断器
    fn should_trip(&self) -> bool {
        let failure_count = self.failure_count.load(Ordering::Relaxed);
        let success_count = self.success_count.load(Ordering::Relaxed);
        let total_requests = failure_count + success_count;

        if total_requests < self.config.minimum_requests as u64 {
            return false;
        }

        let failure_rate = (failure_count as f64 / total_requests as f64) * 100.0;
        failure_rate >= self.config.failure_threshold
    }

    /// 转换到关闭状态
    fn transition_to_closed(&self) {
        *self.state.write() = CircuitBreakerState::Closed;
        self.reset_counters();
        tracing::info!("熔断器 {} 转换到关闭状态", self.service_name);
    }

    /// 转换到打开状态
    fn transition_to_open(&self) {
        *self.state.write() = CircuitBreakerState::Open;
        tracing::warn!("熔断器 {} 转换到打开状态", self.service_name);
    }

    /// 转换到半开状态
    fn transition_to_half_open(&self) {
        *self.state.write() = CircuitBreakerState::HalfOpen;
        self.half_open_requests.store(0, Ordering::Relaxed);
        tracing::info!("熔断器 {} 转换到半开状态", self.service_name);
    }

    /// 重置计数器
    fn reset_counters(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.half_open_requests.store(0, Ordering::Relaxed);
    }

    /// 如果需要的话重置计数器（时间窗口过期）
    fn reset_counters_if_needed(&self) {
        if let Some(last_request) = *self.last_request_time.read() {
            if last_request.elapsed() >= self.config.request_volume_threshold_period {
                self.reset_counters();
            }
        }
    }

    /// 获取当前状态
    pub fn get_state(&self) -> CircuitBreakerState {
        self.state.read().clone()
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            service_name: self.service_name.clone(),
            state: self.get_state(),
            failure_count: self.failure_count.load(Ordering::Relaxed),
            success_count: self.success_count.load(Ordering::Relaxed),
            half_open_requests: self.half_open_requests.load(Ordering::Relaxed),
        }
    }
}

/// 熔断器统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub service_name: String,
    pub state: CircuitBreakerState,
    pub failure_count: u64,
    pub success_count: u64,
    pub half_open_requests: u64,
}

/// 限流器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterConfig {
    /// 每秒允许的请求数
    pub requests_per_second: f64,
    /// 令牌桶容量
    pub bucket_capacity: usize,
    /// 突发请求数量
    pub burst_size: usize,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 1000.0,
            bucket_capacity: 1000,
            burst_size: 100,
        }
    }
}

/// 令牌桶限流器
pub struct TokenBucketRateLimiter {
    config: RateLimiterConfig,
    tokens: Arc<Mutex<f64>>,
    last_refill: Arc<Mutex<Instant>>,
}

impl TokenBucketRateLimiter {
    /// 创建新的限流器
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            tokens: Arc::new(Mutex::new(config.bucket_capacity as f64)),
            last_refill: Arc::new(Mutex::new(Instant::now())),
            config,
        }
    }

    /// 尝试获取令牌
    pub fn try_acquire(&self, tokens: usize) -> bool {
        self.refill_tokens();
        
        let mut current_tokens = self.tokens.lock();
        if *current_tokens >= tokens as f64 {
            *current_tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    /// 等待获取令牌（异步）
    pub async fn acquire(&self, tokens: usize) -> ResilienceResult<()> {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;
        
        while attempts < MAX_ATTEMPTS {
            if self.try_acquire(tokens) {
                return Ok(());
            }
            
            attempts += 1;
            let wait_time = Duration::from_millis(100 * attempts as u64);
            sleep(wait_time).await;
        }
        
        Err(ResilienceError::RateLimitExceeded {
            current_qps: self.config.requests_per_second,
            limit: self.config.requests_per_second,
        })
    }

    /// 重新填充令牌
    fn refill_tokens(&self) {
        let now = Instant::now();
        let mut last_refill = self.last_refill.lock();
        let elapsed = now.duration_since(*last_refill);
        
        if elapsed >= Duration::from_millis(100) { // 每100ms重新填充一次
            let tokens_to_add = elapsed.as_secs_f64() * self.config.requests_per_second;
            let mut current_tokens = self.tokens.lock();
            *current_tokens = (*current_tokens + tokens_to_add).min(self.config.bucket_capacity as f64);
            *last_refill = now;
        }
    }

    /// 获取当前令牌数量
    pub fn get_available_tokens(&self) -> f64 {
        self.refill_tokens();
        *self.tokens.lock()
    }
}

/// 重试策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// 固定延迟
    FixedDelay(Duration),
    /// 指数退避
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
    },
    /// 线性退避
    LinearBackoff {
        initial_delay: Duration,
        increment: Duration,
    },
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: usize,
    /// 重试策略
    pub strategy: RetryStrategy,
    /// 可重试的错误类型
    pub retryable_errors: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: RetryStrategy::default(),
            retryable_errors: vec![
                "连接超时".to_string(),
                "网络错误".to_string(),
                "服务暂时不可用".to_string(),
            ],
        }
    }
}

/// 重试执行器
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// 创建新的重试执行器
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// 执行带重试的异步操作
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut last_error = None;
        
        for attempt in 0..self.config.max_attempts {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    let error_msg = error.to_string();
                    let is_retryable = self.config.retryable_errors.iter()
                        .any(|retryable| error_msg.contains(retryable));
                    
                    if !is_retryable || attempt == self.config.max_attempts - 1 {
                        return Err(error);
                    }
                    
                    let delay = self.calculate_delay(attempt);
                    tracing::warn!(
                        "操作失败，将在 {:?} 后重试 (尝试 {}/{}): {}",
                        delay, attempt + 1, self.config.max_attempts, error_msg
                    );
                    
                    sleep(delay).await;
                    last_error = Some(error);
                }
            }
        }
        
        Err(last_error.unwrap())
    }

    /// 计算重试延迟
    fn calculate_delay(&self, attempt: usize) -> Duration {
        match &self.config.strategy {
            RetryStrategy::FixedDelay(delay) => *delay,
            RetryStrategy::ExponentialBackoff { initial_delay, max_delay, multiplier } => {
                let delay = initial_delay.as_millis() as f64 * multiplier.powi(attempt as i32);
                Duration::from_millis(delay.min(max_delay.as_millis() as f64) as u64)
            }
            RetryStrategy::LinearBackoff { initial_delay, increment } => {
                *initial_delay + *increment * attempt as u32
            }
        }
    }
}

/// 超时包装器
pub struct TimeoutWrapper {
    default_timeout: Duration,
}

impl TimeoutWrapper {
    /// 创建新的超时包装器
    pub fn new(default_timeout: Duration) -> Self {
        Self { default_timeout }
    }

    /// 执行带超时的异步操作
    pub async fn execute<F, Fut, T>(&self, operation: F) -> ResilienceResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.execute_with_timeout(operation, self.default_timeout).await
    }

    /// 执行带自定义超时的异步操作
    pub async fn execute_with_timeout<F, Fut, T>(
        &self,
        operation: F,
        timeout: Duration,
    ) -> ResilienceResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        match tokio::time::timeout(timeout, operation()).await {
            Ok(result) => Ok(result),
            Err(_) => Err(ResilienceError::Timeout { timeout }),
        }
    }
}

/// 资源池
pub struct ResourcePool<T> {
    resources: Arc<Mutex<Vec<T>>>,
    semaphore: Arc<Semaphore>,
    max_size: usize,
    name: String,
}

impl<T> ResourcePool<T> {
    /// 创建新的资源池
    pub fn new(name: String, max_size: usize) -> Self {
        Self {
            resources: Arc::new(Mutex::new(Vec::new())),
            semaphore: Arc::new(Semaphore::new(max_size)),
            max_size,
            name,
        }
    }

    /// 从池中获取资源
    pub async fn acquire(&self) -> ResilienceResult<PooledResource<T>> {
        let _permit = self.semaphore.acquire().await
            .map_err(|_| ResilienceError::ResourcePoolExhausted { 
                resource: self.name.clone() 
            })?;

        let resource = {
            let mut resources = self.resources.lock();
            resources.pop()
        };

        Ok(PooledResource {
            resource,
            pool: self.resources.clone(),
            semaphore: self.semaphore.clone(),
        })
    }

    /// 向池中添加资源
    pub fn add_resource(&self, resource: T) {
        let mut resources = self.resources.lock();
        if resources.len() < self.max_size {
            resources.push(resource);
        }
    }

    /// 获取池状态
    pub fn get_stats(&self) -> PoolStats {
        let resources = self.resources.lock();
        PoolStats {
            name: self.name.clone(),
            available: resources.len(),
            max_size: self.max_size,
            in_use: self.max_size - resources.len(),
        }
    }
}

/// 池化资源
pub struct PooledResource<T> {
    resource: Option<T>,
    pool: Arc<Mutex<Vec<T>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl<T> PooledResource<T> {
    /// 获取资源引用
    pub fn get(&self) -> Option<&T> {
        self.resource.as_ref()
    }

    /// 获取可变资源引用
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.resource.as_mut()
    }

    /// 取出资源
    pub fn take(&mut self) -> Option<T> {
        self.resource.take()
    }
}

impl<T> Drop for PooledResource<T> {
    fn drop(&mut self) {
        if let Some(resource) = self.resource.take() {
            let mut pool = self.pool.lock();
            pool.push(resource);
        }
        // 释放信号量许可，允许其他请求获取资源
        self.semaphore.add_permits(1);
    }
}

/// 资源池统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub name: String,
    pub available: usize,
    pub max_size: usize,
    pub in_use: usize,
}

/// 韧性管理器 - 统一管理所有韧性组件
pub struct ResilienceManager {
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
    rate_limiters: Arc<RwLock<HashMap<String, Arc<TokenBucketRateLimiter>>>>,
    retry_executor: Arc<RetryExecutor>,
    timeout_wrapper: Arc<TimeoutWrapper>,
    resource_pools: Arc<RwLock<HashMap<String, PoolStats>>>,
}

impl ResilienceManager {
    /// 创建新的韧性管理器
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            retry_executor: Arc::new(RetryExecutor::new(RetryConfig::default())),
            timeout_wrapper: Arc::new(TimeoutWrapper::new(default_timeout)),
            resource_pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册熔断器
    pub fn register_circuit_breaker(&self, service_name: String, config: CircuitBreakerConfig) {
        let circuit_breaker = Arc::new(CircuitBreaker::new(service_name.clone(), config));
        let mut circuit_breakers = self.circuit_breakers.write();
        circuit_breakers.insert(service_name, circuit_breaker);
    }

    /// 注册限流器
    pub fn register_rate_limiter(&self, service_name: String, config: RateLimiterConfig) {
        let rate_limiter = Arc::new(TokenBucketRateLimiter::new(config));
        let mut rate_limiters = self.rate_limiters.write();
        rate_limiters.insert(service_name, rate_limiter);
    }

    /// 获取熔断器
    pub fn get_circuit_breaker(&self, service_name: &str) -> Option<Arc<CircuitBreaker>> {
        let circuit_breakers = self.circuit_breakers.read();
        circuit_breakers.get(service_name).cloned()
    }

    /// 获取限流器
    pub fn get_rate_limiter(&self, service_name: &str) -> Option<Arc<TokenBucketRateLimiter>> {
        let rate_limiters = self.rate_limiters.read();
        rate_limiters.get(service_name).cloned()
    }

    /// 获取重试执行器
    pub fn get_retry_executor(&self) -> Arc<RetryExecutor> {
        self.retry_executor.clone()
    }

    /// 获取超时包装器
    pub fn get_timeout_wrapper(&self) -> Arc<TimeoutWrapper> {
        self.timeout_wrapper.clone()
    }

    /// 执行带韧性保护的操作
    pub async fn execute_with_resilience<F, Fut, T, E>(
        &self,
        service_name: &str,
        operation: F,
    ) -> ResilienceResult<T>
    where
        F: Fn() -> Fut + Clone,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display + std::fmt::Debug,
    {
        // 检查熔断器
        if let Some(circuit_breaker) = self.get_circuit_breaker(service_name) {
            if !circuit_breaker.allow_request() {
                return Err(ResilienceError::CircuitBreakerOpen {
                    service: service_name.to_string(),
                });
            }
        }

        // 检查限流器
        if let Some(rate_limiter) = self.get_rate_limiter(service_name) {
            rate_limiter.acquire(1).await?;
        }

        // 执行带重试和超时的操作
        let timeout_wrapper = self.timeout_wrapper.clone();
        let retry_executor = self.retry_executor.clone();
        let circuit_breaker = self.get_circuit_breaker(service_name);

        let result = retry_executor.execute(|| {
            let op = operation.clone();
            let tw = timeout_wrapper.clone();
            async move {
                tw.execute(|| op()).await
                    .map_err(|e| format!("{:?}", e))
                    .and_then(|r| r.map_err(|e| e.to_string()))
            }
        }).await;

        // 更新熔断器状态
        if let Some(cb) = circuit_breaker {
            match &result {
                Ok(_) => cb.record_success(),
                Err(_) => cb.record_failure(),
            }
        }

        result.map_err(|e| ResilienceError::DependencyError {
            service: service_name.to_string(),
            error: e,
        })
    }

    /// 获取所有韧性组件的状态
    pub fn get_resilience_status(&self) -> ResilienceStatus {
        let circuit_breakers = self.circuit_breakers.read();
        let rate_limiters = self.rate_limiters.read();
        let resource_pools = self.resource_pools.read();

        ResilienceStatus {
            circuit_breakers: circuit_breakers.iter()
                .map(|(name, cb)| (name.clone(), cb.get_stats()))
                .collect(),
            rate_limiters: rate_limiters.iter()
                .map(|(name, rl)| (name.clone(), rl.get_available_tokens()))
                .collect(),
            resource_pools: resource_pools.clone(),
            timestamp: SystemTime::now(),
        }
    }
}

/// 韧性状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceStatus {
    pub circuit_breakers: HashMap<String, CircuitBreakerStats>,
    pub rate_limiters: HashMap<String, f64>,
    pub resource_pools: HashMap<String, PoolStats>,
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = CircuitBreakerConfig {
            failure_threshold: 50.0,
            minimum_requests: 2,
            request_volume_threshold_period: Duration::from_secs(60),
            sleep_window: Duration::from_millis(100),
            half_open_max_requests: 1,
        };
        
        let cb = CircuitBreaker::new("test_service".to_string(), config);
        
        // 初始状态应该是关闭的
        assert_eq!(cb.get_state(), CircuitBreakerState::Closed);
        assert!(cb.allow_request());
        
        // 记录失败
        cb.record_failure();
        cb.record_failure();
        
        // 应该触发熔断器打开
        assert_eq!(cb.get_state(), CircuitBreakerState::Open);
        assert!(!cb.allow_request());
        
        // 等待恢复时间
        sleep(Duration::from_millis(150)).await;
        
        // 应该进入半开状态
        assert!(cb.allow_request());
        assert_eq!(cb.get_state(), CircuitBreakerState::HalfOpen);
        
        // 记录成功，应该关闭熔断器
        cb.record_success();
        assert_eq!(cb.get_state(), CircuitBreakerState::Closed);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            bucket_capacity: 10,
            burst_size: 5,
        };
        
        let rl = TokenBucketRateLimiter::new(config);
        
        // 应该能够获取初始令牌
        assert!(rl.try_acquire(5));
        assert!(rl.try_acquire(5));
        
        // 应该无法获取更多令牌
        assert!(!rl.try_acquire(1));
        
        // 等待令牌重新填充
        sleep(Duration::from_millis(200)).await;
        assert!(rl.try_acquire(1));
    }

    #[tokio::test]
    async fn test_retry_executor() {
        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::FixedDelay(Duration::from_millis(10)),
            retryable_errors: vec!["test_error".to_string()],
        };
        
        let executor = RetryExecutor::new(config);
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        
        let result = executor.execute(|| {
            let count = attempt_count.clone();
            async move {
                let current_attempt = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if current_attempt < 3 {
                    Err("test_error")
                } else {
                    Ok("success")
                }
            }
        }).await;
        
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempt_count.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_timeout_wrapper() {
        let wrapper = TimeoutWrapper::new(Duration::from_millis(100));
        
        // 快速操作应该成功
        let result = wrapper.execute(|| async { "success" }).await;
        assert!(result.is_ok());
        
        // 慢操作应该超时
        let result = wrapper.execute(|| async {
            sleep(Duration::from_millis(200)).await;
            "should_timeout"
        }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resource_pool() {
        let pool: ResourcePool<String> = ResourcePool::new("test_pool".to_string(), 2);
        
        pool.add_resource("resource1".to_string());
        pool.add_resource("resource2".to_string());
        
        let resource1 = pool.acquire().await.unwrap();
        let resource2 = pool.acquire().await.unwrap();
        
        assert_eq!(resource1.get(), Some(&"resource1".to_string()));
        assert_eq!(resource2.get(), Some(&"resource2".to_string()));
        
        let stats = pool.get_stats();
        assert_eq!(stats.available, 0);
        assert_eq!(stats.in_use, 2);
    }
}