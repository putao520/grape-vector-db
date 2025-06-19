# 🍇 Grape Vector Database - 3节点集群部署指南

## 📖 概述

3节点集群模式提供了高可用性和数据冗余，是生产环境的推荐部署方式。基于 Raft 共识算法，确保数据一致性和系统可靠性。这种模式适合：

- **生产环境**：需要高可用性和数据安全性
- **企业应用**：关键业务系统的向量搜索
- **大规模服务**：处理大量并发请求
- **数据备份**：自动数据复制和故障恢复
- **负载分担**：分布式查询处理

## ✨ 核心特性

- **🔄 自动故障转移**：节点故障时自动切换
- **📊 数据分片**：智能数据分布和负载均衡
- **🔒 强一致性**：基于Raft算法的数据一致性
- **📈 水平扩展**：支持动态添加和移除节点
- **🛡️ 数据冗余**：多副本数据保护
- **⚖️ 负载均衡**：智能请求分发
- **🔍 分布式搜索**：跨节点并行搜索

## 🏗️ 架构设计

### 集群拓扑

```
        ┌─────────────────┐
        │   Load Balancer │
        │   (Nginx/HAProxy) │
        └─────────┬───────┘
                  │
      ┌───────────┼───────────┐
      │           │           │
┌─────▼─────┐ ┌───▼───┐ ┌─────▼─────┐
│   Node 1  │ │Node 2 │ │   Node 3  │
│ (Leader)  │ │(Follower)│ │(Follower) │
│ Port:6333 │ │Port:6334│ │ Port:6335 │
└─────┬─────┘ └───┬───┘ └─────┬─────┘
      │           │           │
      └───────────┼───────────┘
                  │
        ┌─────────▼─────────┐
        │  Shared Storage   │
        │   (Optional)      │
        └───────────────────┘
```

### 数据分片策略

```
向量数据 -> 哈希函数 -> 分片ID -> 节点分配

示例:
- 分片 0-5:   Node 1 (主) + Node 2,3 (副本)
- 分片 6-11:  Node 2 (主) + Node 1,3 (副本)  
- 分片 12-16: Node 3 (主) + Node 1,2 (副本)
```

## 🚀 快速部署

### Docker Compose 部署

创建 `docker-compose.yml`：

```yaml
version: '3.8'

services:
  grape-node-1:
    image: grape-vector-db:latest
    hostname: grape-node-1
    ports:
      - "6333:6333"   # REST API
      - "6334:6334"   # gRPC API
      - "9090:9090"   # Metrics
    volumes:
      - ./data/node1:/data
      - ./configs/node1.toml:/app/config.toml
    environment:
      - GRAPE_NODE_ID=node-1
      - GRAPE_CLUSTER_ENABLED=true
      - GRAPE_RAFT_ADDRESS=grape-node-1:7000
    command: ["grape-vector-db-server", "--config", "/app/config.toml"]
    networks:
      - grape-cluster
    restart: unless-stopped

  grape-node-2:
    image: grape-vector-db:latest
    hostname: grape-node-2
    ports:
      - "6343:6333"   # REST API
      - "6344:6334"   # gRPC API  
      - "9091:9090"   # Metrics
    volumes:
      - ./data/node2:/data
      - ./configs/node2.toml:/app/config.toml
    environment:
      - GRAPE_NODE_ID=node-2
      - GRAPE_CLUSTER_ENABLED=true
      - GRAPE_RAFT_ADDRESS=grape-node-2:7000
    command: ["grape-vector-db-server", "--config", "/app/config.toml"]
    networks:
      - grape-cluster
    restart: unless-stopped
    depends_on:
      - grape-node-1

  grape-node-3:
    image: grape-vector-db:latest
    hostname: grape-node-3
    ports:
      - "6353:6333"   # REST API
      - "6354:6334"   # gRPC API
      - "9092:9090"   # Metrics
    volumes:
      - ./data/node3:/data
      - ./configs/node3.toml:/app/config.toml
    environment:
      - GRAPE_NODE_ID=node-3
      - GRAPE_CLUSTER_ENABLED=true
      - GRAPE_RAFT_ADDRESS=grape-node-3:7000
    command: ["grape-vector-db-server", "--config", "/app/config.toml"]
    networks:
      - grape-cluster
    restart: unless-stopped
    depends_on:
      - grape-node-1
      - grape-node-2

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "8080:8080"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
    depends_on:
      - grape-node-1
      - grape-node-2
      - grape-node-3
    networks:
      - grape-cluster

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9093:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    networks:
      - grape-cluster

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=grape123
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    networks:
      - grape-cluster

networks:
  grape-cluster:
    driver: bridge

volumes:
  grafana-data:
```

### 节点配置文件

#### Node 1 配置 (`configs/node1.toml`)

```toml
[server]
grpc_port = 6334
rest_port = 6333
data_dir = "/data"
log_level = "info"
worker_threads = 8

[cluster]
enabled = true
node_id = "node-1"
node_address = "grape-node-1:6334"
raft_port = 7000
cluster_token = "grape-cluster-secret-token"

# 初始集群成员
initial_peers = [
    "node-1@grape-node-1:7000",
    "node-2@grape-node-2:7000", 
    "node-3@grape-node-3:7000"
]

# Raft 配置
[raft]
election_timeout_ms = 1000
heartbeat_interval_ms = 250
log_compaction_threshold = 10000
snapshot_interval = 1000
max_append_entries = 100

[sharding]
shard_count = 16
replica_count = 3
rebalance_threshold = 0.1
migration_batch_size = 1000

[vector]
dimension = 768
distance_metric = "cosine"
binary_quantization = true

[storage]
compression_enabled = true
cache_size_mb = 1024
write_buffer_size_mb = 128

[index]
m = 32
ef_construction = 400
ef_search = 200

[monitoring]
prometheus_enabled = true
prometheus_port = 9090
metrics_interval_secs = 10
```

#### Node 2 配置 (`configs/node2.toml`)

```toml
[server]
grpc_port = 6334
rest_port = 6333
data_dir = "/data"
log_level = "info"
worker_threads = 8

[cluster]
enabled = true
node_id = "node-2"
node_address = "grape-node-2:6334"
raft_port = 7000
cluster_token = "grape-cluster-secret-token"

# 加入现有集群
join_cluster = true
seed_nodes = ["node-1@grape-node-1:7000"]

[raft]
election_timeout_ms = 1000
heartbeat_interval_ms = 250
log_compaction_threshold = 10000
snapshot_interval = 1000
max_append_entries = 100

[sharding]
shard_count = 16
replica_count = 3
rebalance_threshold = 0.1
migration_batch_size = 1000

[vector]
dimension = 768
distance_metric = "cosine"
binary_quantization = true

[storage]
compression_enabled = true
cache_size_mb = 1024
write_buffer_size_mb = 128

[index]
m = 32
ef_construction = 400
ef_search = 200

[monitoring]
prometheus_enabled = true
prometheus_port = 9090
metrics_interval_secs = 10
```

#### Node 3 配置 (`configs/node3.toml`)

```toml
[server]
grpc_port = 6334
rest_port = 6333
data_dir = "/data"
log_level = "info"
worker_threads = 8

[cluster]
enabled = true
node_id = "node-3"
node_address = "grape-node-3:6334"
raft_port = 7000
cluster_token = "grape-cluster-secret-token"

# 加入现有集群
join_cluster = true  
seed_nodes = ["node-1@grape-node-1:7000"]

[raft]
election_timeout_ms = 1000
heartbeat_interval_ms = 250
log_compaction_threshold = 10000
snapshot_interval = 1000
max_append_entries = 100

[sharding]
shard_count = 16
replica_count = 3
rebalance_threshold = 0.1
migration_batch_size = 1000

[vector]
dimension = 768
distance_metric = "cosine"
binary_quantization = true

[storage]
compression_enabled = true
cache_size_mb = 1024
write_buffer_size_mb = 128

[index]
m = 32
ef_construction = 400
ef_search = 200

[monitoring]
prometheus_enabled = true
prometheus_port = 9090
metrics_interval_secs = 10
```

### 负载均衡配置

#### Nginx 配置 (`nginx.conf`)

```nginx
events {
    worker_connections 1024;
}

http {
    upstream grape_rest_backend {
        # 健康检查和负载均衡
        server grape-node-1:6333 max_fails=3 fail_timeout=30s;
        server grape-node-2:6333 max_fails=3 fail_timeout=30s;
        server grape-node-3:6333 max_fails=3 fail_timeout=30s;
    }

    upstream grape_grpc_backend {
        server grape-node-1:6334 max_fails=3 fail_timeout=30s;
        server grape-node-2:6334 max_fails=3 fail_timeout=30s;
        server grape-node-3:6334 max_fails=3 fail_timeout=30s;
    }

    # REST API 负载均衡
    server {
        listen 80;
        location / {
            proxy_pass http://grape_rest_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_connect_timeout 30s;
            proxy_send_timeout 30s;
            proxy_read_timeout 30s;
            
            # 健康检查
            health_check uri=/health interval=30s;
        }
    }

    # gRPC 负载均衡
    server {
        listen 8080 http2;
        location / {
            grpc_pass grpc://grape_grpc_backend;
            grpc_connect_timeout 30s;
            grpc_send_timeout 30s;
            grpc_read_timeout 30s;
        }
    }
}
```

### 监控配置

#### Prometheus 配置 (`prometheus.yml`)

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'grape-cluster'
    static_configs:
      - targets: 
        - 'grape-node-1:9090'
        - 'grape-node-2:9090'
        - 'grape-node-3:9090'
    metrics_path: /metrics
    scrape_interval: 10s

  - job_name: 'grape-cluster-raft'
    static_configs:
      - targets:
        - 'grape-node-1:7000'
        - 'grape-node-2:7000'
        - 'grape-node-3:7000'
    metrics_path: /raft/metrics
    scrape_interval: 5s

rule_files:
  - "alert_rules.yml"

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

## 🔧 部署步骤

### 1. 准备环境

```bash
# 创建目录结构
mkdir grape-cluster
cd grape-cluster
mkdir -p {data/{node1,node2,node3},configs,logs}

# 设置权限
chmod 755 data/*/
```

### 2. 配置文件部署

```bash
# 复制配置文件到对应目录
cp node1.toml configs/
cp node2.toml configs/
cp node3.toml configs/
cp nginx.conf ./
cp prometheus.yml ./
```

### 3. 启动集群

```bash
# 启动整个集群
docker-compose up -d

# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f grape-node-1
```

### 4. 验证集群状态

```bash
# 检查集群健康状态
curl http://localhost/health

# 查看集群信息
curl http://localhost/cluster/info

# 检查节点状态
curl http://localhost/cluster/nodes
```

## 🔧 集群管理

### 节点操作

#### 添加新节点

```bash
# 准备新节点配置
cat > configs/node4.toml << EOF
[cluster]
node_id = "node-4"
node_address = "grape-node-4:6334"
join_cluster = true
seed_nodes = ["node-1@grape-node-1:7000"]
EOF

# 启动新节点
docker-compose up -d grape-node-4

# 将节点加入集群
curl -X POST http://localhost/cluster/join \
  -H "Content-Type: application/json" \
  -d '{"node_id": "node-4", "address": "grape-node-4:6334"}'
```

#### 移除节点

```bash
# 优雅移除节点
curl -X POST http://localhost/cluster/leave \
  -H "Content-Type: application/json" \
  -d '{"node_id": "node-3"}'

# 等待数据迁移完成
curl http://localhost/cluster/migration/status

# 停止节点
docker-compose stop grape-node-3
```

### 数据管理

#### 分片重平衡

```bash
# 检查分片分布
curl http://localhost/cluster/shards

# 触发重平衡
curl -X POST http://localhost/cluster/rebalance

# 监控重平衡进度
curl http://localhost/cluster/rebalance/status
```

#### 数据备份

```bash
# 创建集群快照
curl -X POST http://localhost/cluster/snapshot

# 下载备份文件
curl http://localhost/cluster/backup/latest > cluster_backup.tar.gz

# 验证备份完整性
curl http://localhost/cluster/backup/verify
```

## 📊 客户端使用

### 智能客户端

```rust
use grape_vector_db::cluster::ClusterClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建集群客户端
    let client = ClusterClient::new(vec![
        "http://localhost:6333".to_string(),
        "http://localhost:6343".to_string(),
        "http://localhost:6353".to_string(),
    ]).await?;

    // 客户端自动处理负载均衡和故障转移
    let point = Point {
        id: "cluster_doc_1".to_string(),
        vector: vec![0.1; 768],
        payload: [("title".to_string(), "集群文档".into())].into(),
    };

    // 写操作会路由到主节点
    client.upsert_point(point).await?;

    // 读操作会负载均衡到所有节点
    let results = client.search_vectors(&vec![0.1; 768], 10).await?;
    println!("搜索结果: {} 个", results.len());

    Ok(())
}
```

### HTTP 客户端

```bash
# 通过负载均衡器访问
curl -X POST http://localhost/vectors \
  -H "Content-Type: application/json" \
  -d '{
    "id": "http_doc_1",
    "vector": [0.1, 0.2, 0.3],
    "payload": {"title": "HTTP客户端测试"}
  }'

# 搜索请求会自动分发到最优节点
curl -X POST http://localhost/vectors/search \
  -H "Content-Type: application/json" \
  -d '{
    "vector": [0.1, 0.2, 0.3],
    "limit": 10
  }'
```

### Python 集群客户端

```python
import requests
import random
from typing import List, Dict, Any

class GrapeClusterClient:
    def __init__(self, endpoints: List[str]):
        self.endpoints = endpoints
        self.session = requests.Session()
    
    def _get_endpoint(self) -> str:
        """随机选择一个健康的端点"""
        random.shuffle(self.endpoints)
        for endpoint in self.endpoints:
            try:
                response = self.session.get(f"{endpoint}/health", timeout=5)
                if response.status_code == 200:
                    return endpoint
            except:
                continue
        raise Exception("没有可用的端点")
    
    def add_vector(self, vector_id: str, vector: List[float], payload: Dict[str, Any] = None):
        endpoint = self._get_endpoint()
        response = self.session.post(
            f"{endpoint}/vectors",
            json={
                "id": vector_id,
                "vector": vector,
                "payload": payload or {}
            }
        )
        response.raise_for_status()
        return response.json()
    
    def search_vectors(self, query_vector: List[float], limit: int = 10):
        endpoint = self._get_endpoint()
        response = self.session.post(
            f"{endpoint}/vectors/search",
            json={
                "vector": query_vector,
                "limit": limit
            }
        )
        response.raise_for_status()
        return response.json()

# 使用示例
client = GrapeClusterClient([
    "http://localhost:6333",
    "http://localhost:6343", 
    "http://localhost:6353"
])

# 添加向量
client.add_vector("python_doc_1", [0.1] * 768, {"language": "python"})

# 搜索向量
results = client.search_vectors([0.1] * 768, 5)
print(f"找到 {len(results['results'])} 个结果")
```

## 📈 监控和告警

### Grafana 仪表板

关键监控指标：

1. **集群健康状态**
   - 节点在线状态
   - Raft 领导者状态
   - 分片健康状态

2. **性能指标**
   - QPS (每秒查询数)
   - 延迟分布 (P50, P95, P99)
   - 错误率

3. **资源使用**
   - CPU 使用率
   - 内存使用率
   - 磁盘 I/O
   - 网络流量

4. **数据指标**
   - 向量数量
   - 数据分布
   - 缓存命中率

### 告警规则

```yaml
# alert_rules.yml
groups:
  - name: grape_cluster_alerts
    rules:
      - alert: NodeDown
        expr: up{job="grape-cluster"} == 0
        for: 30s
        labels:
          severity: critical
        annotations:
          summary: "Grape节点下线"
          description: "节点 {{ $labels.instance }} 已下线"

      - alert: HighLatency
        expr: grape_query_duration_seconds{quantile="0.95"} > 0.1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "查询延迟过高"
          description: "95%分位延迟超过100ms"

      - alert: MemoryUsageHigh
        expr: grape_memory_usage_bytes / grape_memory_limit_bytes > 0.8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "内存使用率过高"
          description: "内存使用率超过80%"
```

## 🛠️ 故障处理

### 常见故障场景

#### 1. 单节点故障

```bash
# 模拟节点故障
docker-compose stop grape-node-2

# 检查集群状态
curl http://localhost/cluster/info

# 集群应该继续正常工作 (2/3 节点健康)
curl http://localhost/vectors/search -d '{"vector": [0.1], "limit": 5}'

# 恢复节点
docker-compose start grape-node-2
```

#### 2. 网络分区

```bash
# 模拟网络分区 (隔离一个节点)
docker network disconnect grape-cluster_grape-cluster grape-node-3

# 检查集群行为
curl http://localhost/cluster/info

# 恢复网络连接
docker network connect grape-cluster_grape-cluster grape-node-3
```

#### 3. 数据不一致

```bash
# 检查数据一致性
curl http://localhost/cluster/consistency/check

# 如果发现不一致，触发修复
curl -X POST http://localhost/cluster/consistency/repair
```

### 故障恢复流程

1. **检测故障**：监控系统发现节点异常
2. **隔离故障节点**：从负载均衡器移除
3. **数据迁移**：将故障节点的数据迁移到健康节点
4. **服务恢复**：恢复故障节点或添加新节点
5. **数据同步**：确保数据一致性
6. **重新加入**：将恢复的节点重新加入集群

## 📊 性能基准

### 测试环境
- 节点配置: 4核8GB内存，SSD存储
- 网络: 1Gbps 内网
- 数据集: 100万个768维向量

### 基准结果

| 操作类型 | QPS | 延迟 (P95) | 可用性 |
|---------|-----|-----------|--------|
| 写入操作 | 25,000+ | 15ms | 99.9% |
| 搜索操作 | 50,000+ | 8ms | 99.99% |
| 单节点故障 | 18,000+ | 12ms | 100% |
| 网络分区 | 15,000+ | 18ms | 99.9% |

## 🔗 相关链接

- [内嵌模式部署指南](./deployment-embedded-mode.md)
- [单节点部署指南](./deployment-single-node.md)
- [集群运维手册](./cluster-operations.md)
- [性能调优指南](./performance-tuning.md)
- [故障排除指南](./troubleshooting.md)