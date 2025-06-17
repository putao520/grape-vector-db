# 🧪 Grape Vector Database - 测试执行指南

本文档详细说明如何执行 Grape Vector Database 的综合测试方案，包括各种运行模式和测试场景。

## 📋 测试概览

我们的综合测试方案覆盖以下四种运行模式：
- **内嵌模式** (Embedded Mode) - 进程内集成
- **单机模式** (Standalone Mode) - 独立服务
- **3集群模式** (3-Node Cluster) - 基本分布式
- **6集群模式** (6-Node Cluster) - 高可用分布式

## 🚀 快速开始

### 环境要求
```bash
# Rust 版本
rustc 1.70.0+

# 系统要求
- RAM: 至少 4GB (推荐 8GB+)
- 磁盘: 至少 2GB 可用空间
- CPU: 多核处理器 (推荐 4核+)

# 依赖安装
cargo install criterion
```

### 运行所有测试
```bash
# 运行完整测试套件
cargo test --verbose

# 运行特定测试组
cargo test --test embedded_mode_tests
cargo test --test standalone_mode_tests  
cargo test --test raft_comprehensive_tests
cargo test --test resharding_comprehensive_tests
cargo test --test cluster_mode_tests
cargo test --test chaos_engineering_tests
```

## 📦 内嵌模式测试

内嵌模式测试专注于进程内集成和同步API的功能验证。

### 运行内嵌模式测试
```bash
# 完整内嵌模式测试
cargo test --test embedded_mode_tests --verbose

# 特定测试组
cargo test --test embedded_mode_tests embedded_basic_tests
cargo test --test embedded_mode_tests embedded_performance_tests
cargo test --test embedded_mode_tests embedded_concurrency_tests
cargo test --test embedded_mode_tests embedded_resource_tests
```

### 测试覆盖内容
- ✅ 启动关闭性能 (< 1s 启动, < 5s 关闭)
- ✅ 同步API功能完整性
- ✅ 生命周期管理
- ✅ 并发安全性验证
- ✅ 内存泄漏防护
- ✅ 资源自动清理

### 预期结果
```
✅ 内嵌模式启动关闭测试通过
✅ 内嵌模式同步API测试通过  
✅ 内嵌模式生命周期管理测试通过
✅ 内嵌模式并发访问测试通过
✅ 内嵌模式线程安全测试通过
✅ 内嵌模式资源清理测试通过
```

## 🖥️ 单机模式测试

单机模式测试验证独立服务的gRPC接口、REST API兼容性和持久化能力。

### 运行单机模式测试
```bash
# 完整单机模式测试
cargo test --test standalone_mode_tests --verbose

# 特定测试组
cargo test --test standalone_mode_tests standalone_service_tests
cargo test --test standalone_mode_tests standalone_persistence_tests
cargo test --test standalone_mode_tests standalone_performance_tests
```

### 测试覆盖内容
- ✅ gRPC服务接口
- ✅ REST API Qdrant兼容性
- ✅ 数据持久化和恢复
- ✅ 索引重建功能
- ✅ 服务健康监控
- ✅ 高并发处理能力

### 预期结果
```
✅ 单机服务启动测试通过
✅ gRPC接口测试通过
✅ REST API兼容性测试通过
✅ 数据持久化测试通过
✅ 单机模式吞吐量测试通过 (>800 写入/s, >2000 读取/s)
```

## 🗳️ Raft 共识算法测试

深度测试 Raft 共识算法在各种场景下的正确性和可靠性。

### 运行 Raft 测试
```bash
# 完整 Raft 算法测试
cargo test --test raft_comprehensive_tests --verbose

# 特定测试组
cargo test --test raft_comprehensive_tests raft_basic_tests
cargo test --test raft_comprehensive_tests raft_fault_tolerance_tests
cargo test --test raft_comprehensive_tests raft_network_partition_tests
cargo test --test raft_comprehensive_tests raft_edge_case_tests
cargo test --test raft_comprehensive_tests raft_performance_tests
```

### 测试覆盖内容
- ✅ 领导者选举机制
- ✅ 日志复制一致性
- ✅ 节点故障恢复
- ✅ 网络分区处理
- ✅ 脑裂预防
- ✅ 并发选举处理
- ✅ 高吞吐量日志复制

### 预期结果
```
✅ 三节点集群领导者选举测试通过
✅ 六节点集群领导者选举测试通过
✅ 日志复制测试通过
✅ 领导者故障恢复测试通过
✅ 网络分区测试通过
✅ 脑裂预防测试通过
✅ 高吞吐量日志复制测试通过 (>100 ops/sec)
```

## 🔀 Resharding 分片算法测试

全面验证分片算法的分布均匀性、迁移正确性和容错能力。

### 运行分片算法测试
```bash
# 完整分片算法测试
cargo test --test resharding_comprehensive_tests --verbose

# 特定测试组
cargo test --test resharding_comprehensive_tests consistent_hash_tests
cargo test --test resharding_comprehensive_tests shard_migration_tests  
cargo test --test resharding_comprehensive_tests shard_rebalancing_tests
cargo test --test resharding_comprehensive_tests shard_fault_tolerance_tests
```

### 测试覆盖内容
- ✅ 一致性哈希分布均匀性
- ✅ 节点添加/移除时的键重映射
- ✅ 分片迁移数据完整性
- ✅ 并发迁移处理
- ✅ 负载重平衡
- ✅ 分片副本一致性
- ✅ 部分分片故障容错

### 预期结果
```
✅ 一致性哈希分布测试通过 (分布偏差 < 20%)
✅ 节点添加一致性测试通过 (重映射率 20%-35%)
✅ 分片迁移测试通过
✅ 自动重平衡测试通过 (负载偏差 < 15%)
✅ 分片副本一致性测试通过
✅ 大规模重分片测试通过 (1000 文档 < 2s)
```

## 🏭 集群模式测试

测试 3 节点和 6 节点集群的分布式协调、负载均衡和高可用能力。

### 运行集群模式测试
```bash
# 完整集群模式测试
cargo test --test cluster_mode_tests --verbose

# 特定测试组
cargo test --test cluster_mode_tests three_node_cluster_tests
cargo test --test cluster_mode_tests six_node_cluster_tests
cargo test --test cluster_mode_tests cluster_fault_injection_tests
```

### 测试覆盖内容

#### 3节点集群测试
- ✅ 基本共识功能
- ✅ 单节点故障容错 (2/3 可用)
- ✅ 网络分区处理
- ✅ 领导者选举稳定性

#### 6节点集群测试  
- ✅ 高可用性 (4/6 节点故障容错)
- ✅ 大规模并发操作 (500+ 并发)
- ✅ 动态扩缩容
- ✅ 复杂故障场景处理

### 预期结果
```
✅ 3节点集群基本共识测试通过
✅ 3节点集群单节点故障容错测试通过  
✅ 6节点集群高可用性测试通过
✅ 6节点集群大规模并发操作测试通过 (成功率 >90%, 延迟 <100ms)
✅ 集群扩容测试通过
✅ 复杂故障场景测试通过 (数据完整性 >90%)
```

## 🌪️ 混沌工程测试

系统级故障注入测试，验证在极端条件下的系统行为。

### 运行混沌工程测试
```bash
# 完整混沌工程测试 (需要更长时间)
cargo test --test chaos_engineering_tests --verbose

# 特定混沌测试
cargo test --test chaos_engineering_tests test_disaster_recovery
cargo test --test chaos_engineering_tests test_byzantine_failures
cargo test --test chaos_engineering_tests test_extreme_load_chaos
```

### 测试覆盖内容
- ✅ 灾难恢复 (4/6 节点同时故障)
- ✅ 拜占庭故障处理
- ✅ 级联故障预防
- ✅ 数据损坏场景
- ✅ 极限负载测试
- ✅ 网络混沌注入

### 预期结果
```
✅ 灾难恢复测试通过 (降级模式 >30% 可访问, 恢复率 >95%)
✅ 拜占庭故障测试通过 (系统保持一致性)
✅ 级联故障测试通过 (阻止完全失效)
✅ 极限负载混沌测试通过 (可用性 >60%, 一致性违反 ≤5)
```

## ⚡ 性能基准测试

使用 Criterion 框架进行详细的性能分析和对比。

### 运行性能基准测试
```bash
# 运行所有性能基准
cargo bench --bench comprehensive_benchmarks

# 特定基准测试
cargo bench --bench comprehensive_benchmarks embedded_mode
cargo bench --bench comprehensive_benchmarks raft_algorithm
cargo bench --bench comprehensive_benchmarks resharding_algorithm
```

### 基准测试内容
- ⚡ 内嵌模式文档插入性能
- ⚡ 单机模式 gRPC/REST 性能
- ⚡ 集群模式共识性能
- ⚡ Raft 领导者选举时间
- ⚡ 分片算法一致性哈希性能
- ⚡ 内存使用效率

### 性能目标
```
⚡ 内嵌模式插入: >1000 docs/sec
⚡ 单机模式写入: >800 ops/sec  
⚡ 单机模式读取: >2000 ops/sec
⚡ Raft 共识延迟: <50ms
⚡ 领导者选举: <300ms
⚡ 一致性哈希: >10000 keys/sec
```

## 🔍 测试调试

### 启用详细日志
```bash
# 设置日志级别
export RUST_LOG=debug
cargo test --test raft_comprehensive_tests

# 特定模块日志
export RUST_LOG=grape_vector_db::distributed::raft=trace
cargo test --test raft_comprehensive_tests
```

### 单独运行特定测试
```bash
# 运行单个测试函数
cargo test --test raft_comprehensive_tests test_leader_election_three_nodes

# 运行测试并显示输出
cargo test --test cluster_mode_tests test_high_availability_cluster -- --nocapture
```

### 性能分析
```bash
# 生成性能报告
cargo bench --bench comprehensive_benchmarks
# 报告位置: target/criterion/report/index.html
```

## 📊 CI/CD 集成

### GitHub Actions 工作流
```bash
# 手动触发完整测试
gh workflow run comprehensive-tests.yml

# 查看测试状态
gh run list --workflow=comprehensive-tests.yml
```

### 测试覆盖率
```bash
# 安装覆盖率工具
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --out Html --output-dir target/coverage
```

## 🎯 成功标准

### 功能正确性
- [ ] 所有单元测试通过
- [ ] 所有集成测试通过
- [ ] Raft 算法测试 100% 通过
- [ ] 分片算法测试 100% 通过

### 性能指标
- [ ] 内嵌模式启动时间 < 1s
- [ ] 单机模式响应时间 < 10ms (P95)
- [ ] 3集群共识延迟 < 50ms (P95)  
- [ ] 6集群吞吐量 > 10k QPS

### 可靠性标准
- [ ] 混沌测试可用性 > 99%
- [ ] 故障场景数据丢失 = 0
- [ ] 最终一致性 = 100%

## 🛠️ 故障排除

### 常见问题

1. **测试超时**
   ```bash
   # 增加超时时间
   cargo test --test cluster_mode_tests -- --test-threads=1
   ```

2. **内存不足**
   ```bash
   # 减少并发测试
   export TEST_CONCURRENCY=2
   cargo test --test chaos_engineering_tests
   ```

3. **网络端口冲突**
   ```bash
   # 使用随机端口
   export TEST_PORT_RANGE=9000-9999
   cargo test --test standalone_mode_tests
   ```

4. **临时文件清理**
   ```bash
   # 清理测试产生的临时文件
   rm -rf /tmp/grape_test_*
   ```

### 调试技巧

1. **启用详细输出**
   ```bash
   RUST_LOG=trace cargo test --test raft_comprehensive_tests -- --nocapture
   ```

2. **单步调试**
   ```bash
   # 设置断点并调试
   rust-gdb target/debug/deps/raft_comprehensive_tests-*
   ```

3. **内存检查**
   ```bash
   # 使用 Valgrind 检查内存泄漏
   valgrind --tool=memcheck cargo test --test embedded_mode_tests
   ```

## 📈 持续改进

### 性能监控
- 建立性能基线数据库
- 设置性能回归告警
- 定期优化热点路径

### 测试扩展
- 添加更多边界条件测试
- 增加真实场景模拟
- 扩展混沌工程测试场景

### 文档更新
- 保持测试文档与代码同步
- 记录新发现的测试场景
- 分享最佳实践

---

通过这个综合测试方案，我们确保 Grape Vector Database 在所有运行模式下都能提供可靠、高性能的服务，特别是验证了 Raft 共识算法和 Resharding 分片算法的正确性。