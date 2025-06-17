/// 混沌工程引擎
/// 
/// 提供系统级的故障注入和混沌测试能力，验证系统在异常条件下的行为

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::test_framework::{NetworkSimulator, NetworkChaos};

/// 混沌工程引擎
pub struct ChaosEngine {
    /// 网络模拟器
    network_simulator: Arc<NetworkSimulator>,
    /// 当前运行的实验
    running_experiments: Arc<RwLock<Vec<RunningExperiment>>>,
    /// 实验结果收集器
    metrics_collector: Arc<RwLock<MetricsCollector>>,
}

/// 混沌实验配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosExperiment {
    /// 实验名称
    pub name: String,
    /// 实验持续时间
    pub duration: Duration,
    /// 节点故障率 (0.0-1.0)
    pub node_failure_rate: f64,
    /// 网络故障率 (0.0-1.0)
    pub network_failure_rate: f64,
    /// 故障恢复时间
    pub recovery_time: Duration,
    /// 网络混沌配置
    pub network_chaos: Option<NetworkChaos>,
    /// 工作负载配置
    pub workload: Option<WorkloadConfig>,
}

/// 工作负载配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadConfig {
    /// 读取率 (QPS)
    pub read_rate: u32,
    /// 写入率 (QPS)
    pub write_rate: u32,
    /// 工作负载持续时间
    pub duration: Duration,
    /// 并发连接数
    pub concurrent_connections: u32,
}

/// 运行中的实验
struct RunningExperiment {
    id: String,
    config: ChaosExperiment,
    start_time: Instant,
    status: ExperimentStatus,
}

/// 实验状态
#[derive(Debug, Clone, PartialEq)]
enum ExperimentStatus {
    Running,
    Completed,
    Failed,
    Aborted,
}

/// 指标收集器
struct MetricsCollector {
    /// 系统可用性指标
    availability_metrics: Vec<AvailabilityPoint>,
    /// 性能指标
    performance_metrics: Vec<PerformancePoint>,
    /// 一致性指标
    consistency_metrics: Vec<ConsistencyPoint>,
    /// 错误统计
    error_stats: ErrorStats,
}

/// 可用性数据点
#[derive(Debug, Clone)]
struct AvailabilityPoint {
    timestamp: Instant,
    is_available: bool,
    active_nodes: u32,
    total_nodes: u32,
}

/// 性能数据点
#[derive(Debug, Clone)]
struct PerformancePoint {
    timestamp: Instant,
    read_latency_ms: f64,
    write_latency_ms: f64,
    throughput_qps: f64,
}

/// 一致性数据点
#[derive(Debug, Clone)]
struct ConsistencyPoint {
    timestamp: Instant,
    consistency_violations: u32,
    successful_reads: u32,
    stale_reads: u32,
}

/// 错误统计
#[derive(Debug, Clone, Default)]
struct ErrorStats {
    total_errors: u32,
    timeout_errors: u32,
    network_errors: u32,
    consistency_errors: u32,
    unknown_errors: u32,
}

impl ChaosEngine {
    /// 创建新的混沌工程引擎
    pub fn new(network_simulator: Arc<NetworkSimulator>) -> Self {
        Self {
            network_simulator,
            running_experiments: Arc::new(RwLock::new(Vec::new())),
            metrics_collector: Arc::new(RwLock::new(MetricsCollector::new())),
        }
    }
    
    /// 运行混沌实验
    pub async fn run_experiment(&self, experiment: ChaosExperiment) -> Result<ExperimentResult> {
        let experiment_id = uuid::Uuid::new_v4().to_string();
        tracing::info!("开始混沌实验: {} ({})", experiment.name, experiment_id);
        
        // 记录实验开始
        {
            let mut experiments = self.running_experiments.write().await;
            experiments.push(RunningExperiment {
                id: experiment_id.clone(),
                config: experiment.clone(),
                start_time: Instant::now(),
                status: ExperimentStatus::Running,
            });
        }
        
        // 启动指标收集
        let metrics_task = self.start_metrics_collection();
        
        // 运行实验
        let result = self.execute_experiment(&experiment).await;
        
        // 停止指标收集
        metrics_task.abort();
        
        // 更新实验状态
        {
            let mut experiments = self.running_experiments.write().await;
            if let Some(exp) = experiments.iter_mut().find(|e| e.id == experiment_id) {
                exp.status = if result.is_ok() {
                    ExperimentStatus::Completed
                } else {
                    ExperimentStatus::Failed
                };
            }
        }
        
        // 生成实验结果
        let metrics = self.collect_experiment_metrics().await;
        
        let experiment_result = ExperimentResult {
            experiment_id,
            experiment_name: experiment.name.clone(),
            duration: experiment.duration,
            success: result.is_ok(),
            metrics,
            error_message: result.err().map(|e| e.to_string()),
        };
        
        tracing::info!("混沌实验完成: {} - {}", 
            experiment.name, 
            if experiment_result.success { "成功" } else { "失败" }
        );
        
        Ok(experiment_result)
    }
    
    /// 执行混沌实验
    async fn execute_experiment(&self, experiment: &ChaosExperiment) -> Result<()> {
        let experiment_duration = experiment.duration;
        let start_time = Instant::now();
        
        // 创建故障注入任务
        let failure_injection_task = self.start_failure_injection(experiment);
        
        // 创建工作负载任务
        let workload_task = if let Some(workload) = &experiment.workload {
            Some(self.start_workload(workload.clone()))
        } else {
            None
        };
        
        // 等待实验完成
        while start_time.elapsed() < experiment_duration {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // 检查是否需要中止实验
            if self.should_abort_experiment().await {
                tracing::warn!("混沌实验被中止");
                break;
            }
        }
        
        // 清理故障注入
        failure_injection_task.abort();
        if let Some(task) = workload_task {
            task.abort();
        }
        
        // 恢复系统到正常状态
        self.cleanup_chaos_effects().await?;
        
        Ok(())
    }
    
    /// 开始故障注入
    fn start_failure_injection(&self, experiment: &ChaosExperiment) -> tokio::task::JoinHandle<()> {
        let network_simulator = self.network_simulator.clone();
        let node_failure_rate = experiment.node_failure_rate;
        let network_failure_rate = experiment.network_failure_rate;
        let recovery_time = experiment.recovery_time;
        let network_chaos = experiment.network_chaos.clone();
        
        tokio::spawn(async move {
            let mut last_failure_time = Instant::now();
            
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                // 随机节点故障
                if fastrand::f64() < node_failure_rate {
                    let node_id = format!("node_{}", fastrand::u32(0..6));
                    network_simulator.fail_node(node_id.clone()).await;
                    
                    // 安排恢复
                    let simulator_clone = network_simulator.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(recovery_time).await;
                        simulator_clone.recover_node(&node_id).await;
                    });
                    
                    last_failure_time = Instant::now();
                }
                
                // 网络故障
                if fastrand::f64() < network_failure_rate {
                    if let Some(chaos) = &network_chaos {
                        let _ = network_simulator.inject_chaos(
                            chaos.clone(), 
                            Duration::from_millis(200)
                        ).await;
                    }
                }
                
                // 避免过于频繁的故障注入
                if last_failure_time.elapsed() < Duration::from_secs(1) {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }
        })
    }
    
    /// 开始工作负载
    fn start_workload(&self, workload: WorkloadConfig) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let start_time = Instant::now();
            let read_interval = Duration::from_millis(1000 / workload.read_rate as u64);
            let write_interval = Duration::from_millis(1000 / workload.write_rate as u64);
            
            let mut read_counter = 0;
            let mut write_counter = 0;
            
            while start_time.elapsed() < workload.duration {
                // 执行读操作
                if read_counter * read_interval.as_millis() <= start_time.elapsed().as_millis() {
                    // 模拟读操作
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    read_counter += 1;
                }
                
                // 执行写操作
                if write_counter * write_interval.as_millis() <= start_time.elapsed().as_millis() {
                    // 模拟写操作
                    tokio::time::sleep(Duration::from_millis(2)).await;
                    write_counter += 1;
                }
                
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            tracing::info!("工作负载完成: {} 读, {} 写", read_counter, write_counter);
        })
    }
    
    /// 开始指标收集
    fn start_metrics_collection(&self) -> tokio::task::JoinHandle<()> {
        let metrics_collector = self.metrics_collector.clone();
        let network_simulator = self.network_simulator.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                // 收集可用性指标
                let is_healthy = network_simulator.is_network_healthy().await;
                let stats = network_simulator.get_network_stats().await;
                
                {
                    let mut collector = metrics_collector.write().await;
                    collector.availability_metrics.push(AvailabilityPoint {
                        timestamp: Instant::now(),
                        is_available: is_healthy,
                        active_nodes: 6 - stats.failed_node_count as u32,
                        total_nodes: 6,
                    });
                    
                    // 模拟性能指标
                    collector.performance_metrics.push(PerformancePoint {
                        timestamp: Instant::now(),
                        read_latency_ms: if is_healthy { 5.0 } else { 50.0 },
                        write_latency_ms: if is_healthy { 10.0 } else { 100.0 },
                        throughput_qps: if is_healthy { 1000.0 } else { 100.0 },
                    });
                    
                    // 模拟一致性指标
                    collector.consistency_metrics.push(ConsistencyPoint {
                        timestamp: Instant::now(),
                        consistency_violations: if is_healthy { 0 } else { 1 },
                        successful_reads: if is_healthy { 100 } else { 80 },
                        stale_reads: if is_healthy { 0 } else { 20 },
                    });
                }
            }
        })
    }
    
    /// 检查是否应该中止实验
    async fn should_abort_experiment(&self) -> bool {
        // 简化实现：目前不支持中止
        false
    }
    
    /// 清理混沌效果
    async fn cleanup_chaos_effects(&self) -> Result<()> {
        // 愈合所有网络分区
        self.network_simulator.heal_all_partitions().await;
        
        // 恢复所有故障节点
        for i in 0..6 {
            let node_id = format!("node_{}", i);
            self.network_simulator.recover_node(&node_id).await;
        }
        
        tracing::info!("清理混沌效果完成");
        Ok(())
    }
    
    /// 收集实验指标
    async fn collect_experiment_metrics(&self) -> ExperimentMetrics {
        let collector = self.metrics_collector.read().await;
        
        // 计算可用性
        let total_points = collector.availability_metrics.len();
        let available_points = collector.availability_metrics.iter()
            .filter(|p| p.is_available)
            .count();
        let availability = if total_points > 0 {
            available_points as f64 / total_points as f64
        } else {
            0.0
        };
        
        // 计算平均延迟
        let avg_read_latency = if collector.performance_metrics.is_empty() {
            0.0
        } else {
            collector.performance_metrics.iter()
                .map(|p| p.read_latency_ms)
                .sum::<f64>() / collector.performance_metrics.len() as f64
        };
        
        let avg_write_latency = if collector.performance_metrics.is_empty() {
            0.0
        } else {
            collector.performance_metrics.iter()
                .map(|p| p.write_latency_ms)
                .sum::<f64>() / collector.performance_metrics.len() as f64
        };
        
        // 计算一致性违反
        let consistency_violations = collector.consistency_metrics.iter()
            .map(|p| p.consistency_violations)
            .sum();
        
        ExperimentMetrics {
            availability,
            avg_read_latency_ms: avg_read_latency,
            avg_write_latency_ms: avg_write_latency,
            consistency_violations,
            total_data_points: total_points,
        }
    }
}

/// 实验结果
#[derive(Debug, Clone)]
pub struct ExperimentResult {
    pub experiment_id: String,
    pub experiment_name: String,
    pub duration: Duration,
    pub success: bool,
    pub metrics: ExperimentMetrics,
    pub error_message: Option<String>,
}

/// 实验指标
#[derive(Debug, Clone)]
pub struct ExperimentMetrics {
    /// 系统可用性 (0.0-1.0)
    pub availability: f64,
    /// 平均读延迟 (毫秒)
    pub avg_read_latency_ms: f64,
    /// 平均写延迟 (毫秒)
    pub avg_write_latency_ms: f64,
    /// 一致性违反次数
    pub consistency_violations: u32,
    /// 总数据点数
    pub total_data_points: usize,
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            availability_metrics: Vec::new(),
            performance_metrics: Vec::new(),
            consistency_metrics: Vec::new(),
            error_stats: ErrorStats::default(),
        }
    }
}

/// 混沌实验构建器
pub struct ChaosExperimentBuilder {
    name: String,
    duration: Duration,
    node_failure_rate: f64,
    network_failure_rate: f64,
    recovery_time: Duration,
    network_chaos: Option<NetworkChaos>,
    workload: Option<WorkloadConfig>,
}

impl ChaosExperimentBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            duration: Duration::from_secs(60), // 1 minute
            node_failure_rate: 0.1,
            network_failure_rate: 0.1,
            recovery_time: Duration::from_secs(30),
            network_chaos: None,
            workload: None,
        }
    }
    
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }
    
    pub fn with_failure_rate(mut self, node_rate: f64, network_rate: f64) -> Self {
        self.node_failure_rate = node_rate;
        self.network_failure_rate = network_rate;
        self
    }
    
    pub fn with_recovery_time(mut self, recovery_time: Duration) -> Self {
        self.recovery_time = recovery_time;
        self
    }
    
    pub fn with_network_chaos(mut self, chaos: NetworkChaos) -> Self {
        self.network_chaos = Some(chaos);
        self
    }
    
    pub fn with_workload(mut self, workload: WorkloadConfig) -> Self {
        self.workload = Some(workload);
        self
    }
    
    pub fn build(self) -> ChaosExperiment {
        ChaosExperiment {
            name: self.name,
            duration: self.duration,
            node_failure_rate: self.node_failure_rate,
            network_failure_rate: self.network_failure_rate,
            recovery_time: self.recovery_time,
            network_chaos: self.network_chaos,
            workload: self.workload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_chaos_engine_creation() {
        let network_simulator = Arc::new(NetworkSimulator::new());
        let chaos_engine = ChaosEngine::new(network_simulator);
        
        // 验证引擎创建成功
        assert_eq!(chaos_engine.running_experiments.read().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_chaos_experiment_builder() {
        let experiment = ChaosExperimentBuilder::new("test_experiment")
            .with_duration(Duration::from_secs(10))
            .with_failure_rate(0.2, 0.1)
            .with_recovery_time(Duration::from_secs(5))
            .build();
        
        assert_eq!(experiment.name, "test_experiment");
        assert_eq!(experiment.duration, Duration::from_secs(10));
        assert_eq!(experiment.node_failure_rate, 0.2);
        assert_eq!(experiment.network_failure_rate, 0.1);
        assert_eq!(experiment.recovery_time, Duration::from_secs(5));
    }
    
    #[tokio::test]
    async fn test_simple_chaos_experiment() {
        let network_simulator = Arc::new(NetworkSimulator::new());
        let chaos_engine = ChaosEngine::new(network_simulator);
        
        let experiment = ChaosExperimentBuilder::new("simple_test")
            .with_duration(Duration::from_millis(100))
            .with_failure_rate(0.1, 0.1)
            .build();
        
        let result = chaos_engine.run_experiment(experiment).await.unwrap();
        
        assert!(result.success);
        assert_eq!(result.experiment_name, "simple_test");
        assert!(result.metrics.total_data_points > 0);
    }
}