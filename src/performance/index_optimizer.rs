use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

use crate::{
    index::{IndexStats, VectorIndex},
    types::VectorDbError,
};

/// 索引优化器配置
#[derive(Debug, Clone)]
pub struct IndexOptimizerConfig {
    /// 优化间隔（秒）
    pub optimization_interval_seconds: u64,
    /// 自动优化阈值
    pub auto_optimization_threshold: usize,
    /// 是否启用自动优化
    pub enable_auto_optimization: bool,
    /// 最大优化时间（秒）
    pub max_optimization_time_seconds: u64,
}

impl Default for IndexOptimizerConfig {
    fn default() -> Self {
        Self {
            optimization_interval_seconds: 3600, // 1小时
            auto_optimization_threshold: 10000,
            enable_auto_optimization: true,
            max_optimization_time_seconds: 300, // 5分钟
        }
    }
}

/// 索引优化器
pub struct IndexOptimizer {
    config: IndexOptimizerConfig,
    last_optimization: Arc<RwLock<Option<Instant>>>,
}

impl IndexOptimizer {
    /// 创建新的索引优化器
    pub fn new(config: IndexOptimizerConfig) -> Self {
        Self {
            config,
            last_optimization: Arc::new(RwLock::new(None)),
        }
    }

    /// 检查是否需要优化
    pub fn should_optimize(&self, stats: &IndexStats) -> bool {
        if !self.config.enable_auto_optimization {
            return false;
        }

        // 检查向量数量阈值
        if stats.vector_count >= self.config.auto_optimization_threshold {
            return true;
        }

        // 检查时间间隔
        if let Some(last_opt) = *self.last_optimization.read() {
            let elapsed = last_opt.elapsed().as_secs();
            if elapsed >= self.config.optimization_interval_seconds {
                return true;
            }
        } else {
            // 从未优化过
            return true;
        }

        false
    }

    /// 执行索引优化
    pub async fn optimize_index(&self, index: &mut dyn VectorIndex) -> Result<(), VectorDbError> {
        let start_time = Instant::now();
        info!("开始索引优化");

        // 执行优化
        index.optimize()?;

        // 更新最后优化时间
        *self.last_optimization.write() = Some(start_time);

        let elapsed = start_time.elapsed();
        info!("索引优化完成，耗时: {:?}", elapsed);

        Ok(())
    }

    /// 获取优化统计信息
    pub fn get_optimization_stats(&self) -> OptimizationStats {
        let last_optimization = *self.last_optimization.read();

        OptimizationStats {
            last_optimization_time: last_optimization,
            next_optimization_due: last_optimization
                .map(|t| t + Duration::from_secs(self.config.optimization_interval_seconds)),
            auto_optimization_enabled: self.config.enable_auto_optimization,
        }
    }

    /// 强制执行优化
    pub async fn force_optimize(&self, index: &mut dyn VectorIndex) -> Result<(), VectorDbError> {
        self.optimize_index(index).await
    }

    /// 获取配置
    pub fn get_config(&self) -> &IndexOptimizerConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: IndexOptimizerConfig) {
        self.config = config;
    }
}

/// 优化统计信息
#[derive(Debug, Clone)]
pub struct OptimizationStats {
    /// 最后优化时间
    pub last_optimization_time: Option<Instant>,
    /// 下次优化到期时间
    pub next_optimization_due: Option<Instant>,
    /// 是否启用自动优化
    pub auto_optimization_enabled: bool,
}

/// 优化结果
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub duration: Duration,
    pub memory_before: u64,
    pub memory_after: u64,
    pub memory_saved: u64,
    pub performance_improvement: f32,
    pub operations_performed: Vec<String>,
}

/// 内部优化结果
#[derive(Debug)]
struct InternalOptimizationResult {
    operations_performed: Vec<String>,
    performance_improvement: f32,
}

impl Default for IndexOptimizer {
    fn default() -> Self {
        Self::new(IndexOptimizerConfig::default())
    }
}
