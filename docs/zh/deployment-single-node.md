# 🍇 Grape Vector Database - 单节点部署指南

## 📖 概述

单节点模式将 Grape Vector Database 作为独立的服务运行，通过 gRPC 和 REST API 提供向量数据库功能。这种模式适合：

- **微服务架构**：作为独立的向量搜索服务
- **API服务**：为多个客户端提供向量搜索
- **开发测试**：搭建开发和测试环境  
- **原型验证**：快速部署和验证方案
- **轻量级生产**：小规模生产环境

## ✨ 核心特性

- **🌐 gRPC API**：高性能的二进制协议
- **🔗 REST API**：标准的HTTP接口，兼容Qdrant
- **🔄 热重载**：支持配置热更新
- **📊 监控指标**：内置Prometheus指标
- **🛡️ 健康检查**：HTTP健康检查端点
- **📝 日志系统**：结构化日志输出
- **🔐 认证授权**：API密钥和RBAC支持

## 🚀 快速启动

### 使用预构建二进制

```bash
# 下载预构建二进制文件
wget https://github.com/putao520/grape-vector-db/releases/latest/download/grape-vector-db-linux-amd64.tar.gz
tar xzf grape-vector-db-linux-amd64.tar.gz

# 创建配置文件
cat > config.toml << EOF
[server]
grpc_port = 6334
rest_port = 6333
data_dir = "./data"
log_level = "info"

[vector]
dimension = 768
distance_metric = "cosine"

[storage]
compression_enabled = true
cache_size_mb = 512
EOF

# 启动服务
./grape-vector-db --config config.toml
```

### 使用 Docker

```bash
# 创建数据目录
mkdir -p ./grape_data

# 运行 Docker 容器
docker run -d \
  --name grape-vector-db \
  -p 6333:6333 \
  -p 6334:6334 \
  -v ./grape_data:/data \
  -v ./config.toml:/app/config.toml \
  grape-vector-db:latest
```

### 从源码编译

```bash
# 克隆仓库
git clone https://github.com/putao520/grape-vector-db.git
cd grape-vector-db

# 编译发布版本
cargo build --release --bin grape-vector-db-server

# 启动服务
./target/release/grape-vector-db-server --config config.toml
```

## ⚙️ 配置文件详解

### 完整配置示例

```toml
# config.toml - 完整配置文件

[server]
# gRPC 服务端口
grpc_port = 6334
# REST API 端口  
rest_port = 6333
# 数据存储目录
data_dir = "./data"
# 日志级别: trace, debug, info, warn, error
log_level = "info"
# 日志格式: json, pretty
log_format = "pretty"
# 工作线程数量（默认为CPU核心数）
worker_threads = 8
# 最大并发连接数
max_connections = 1000
# 请求超时时间（秒）
request_timeout_secs = 30

[vector]
# 向量维度
dimension = 768
# 距离度量: cosine, euclidean, dot_product
distance_metric = "cosine"
# 是否启用二进制量化
binary_quantization = true
# 量化阈值
quantization_threshold = 0.5

[storage]
# 是否启用压缩
compression_enabled = true
# 缓存大小（MB）
cache_size_mb = 512
# 写缓冲大小（MB）
write_buffer_size_mb = 64
# 最大写缓冲数量
max_write_buffer_number = 4
# 目标文件大小（MB）
target_file_size_mb = 128
# 布隆过滤器参数
bloom_filter_bits_per_key = 10

[index]
# HNSW参数M
m = 32
# 构建时ef参数
ef_construction = 400
# 搜索时ef参数
ef_search = 200
# 最大M值
max_m = 64
# 最大层数
max_level = 16

[auth]
# 是否启用API密钥认证
enabled = false
# API密钥列表
api_keys = [
    "grape-api-key-admin",
    "grape-api-key-readonly"
]
# 只读API密钥
readonly_keys = [
    "grape-api-key-readonly"
]

[monitoring]
# 是否启用Prometheus指标
prometheus_enabled = true
# Prometheus指标端口
prometheus_port = 9090
# 健康检查端点
health_check_path = "/health"
# 指标收集间隔（秒）
metrics_interval_secs = 10

[limits]
# 最大向量数量
max_vectors = 10000000
# 最大批量操作大小
max_batch_size = 1000
# 最大搜索结果数量
max_search_results = 1000
# 最大载荷大小（字节）
max_payload_size_bytes = 65536

[optimization]
# 预热模式
warmup_enabled = true
# 自动压缩
auto_compaction = true
# 压缩间隔（小时）
compaction_interval_hours = 24
# 内存使用阈值（MB）
memory_threshold_mb = 2048
```

### 环境变量配置

```bash
# 服务配置
export GRAPE_GRPC_PORT=6334
export GRAPE_REST_PORT=6333
export GRAPE_DATA_DIR="./data"
export GRAPE_LOG_LEVEL="info"

# 向量配置
export GRAPE_VECTOR_DIMENSION=768
export GRAPE_DISTANCE_METRIC="cosine"

# 存储配置
export GRAPE_COMPRESSION_ENABLED=true
export GRAPE_CACHE_SIZE_MB=512

# 认证配置
export GRAPE_AUTH_ENABLED=true
export GRAPE_API_KEY="your-secret-api-key"
```

## 🔌 API 使用指南

### gRPC API

#### 客户端连接

```rust
use grape_vector_db::client::GrapeVectorDbClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 连接到gRPC服务
    let mut client = GrapeVectorDbClient::connect("http://localhost:6334").await?;
    
    // 添加向量
    let request = tonic::Request::new(UpsertVectorRequest {
        id: "doc_1".to_string(),
        vector: vec![0.1, 0.2, 0.3], // 简化示例
        payload: HashMap::new(),
    });
    
    let response = client.upsert_vector(request).await?;
    println!("向量添加成功: {:?}", response);
    
    // 搜索向量
    let search_request = tonic::Request::new(SearchVectorRequest {
        vector: vec![0.1, 0.2, 0.3],
        limit: 10,
        filter: None,
    });
    
    let search_response = client.search_vectors(search_request).await?;
    println!("搜索结果: {:?}", search_response);
    
    Ok(())
}
```

#### Python客户端

```python
import grpc
from grape_vector_db_pb2 import *
from grape_vector_db_pb2_grpc import *

# 连接到服务
channel = grpc.insecure_channel('localhost:6334')
client = VectorDbServiceStub(channel)

# 添加向量
request = UpsertVectorRequest(
    id="python_doc_1",
    vector=[0.1, 0.2, 0.3, 0.4],
    payload={"title": "Python示例", "language": "zh"}
)

response = client.UpsertVector(request)
print(f"向量添加成功: {response}")

# 搜索向量
search_request = SearchVectorRequest(
    vector=[0.1, 0.2, 0.3, 0.4],
    limit=5
)

search_response = client.SearchVectors(search_request)
print(f"搜索结果数量: {len(search_response.results)}")
```

### REST API

#### 基本操作

```bash
# 健康检查
curl http://localhost:6333/health

# 添加向量
curl -X POST http://localhost:6333/vectors \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer grape-api-key-admin" \
  -d '{
    "id": "rest_doc_1",
    "vector": [0.1, 0.2, 0.3],
    "payload": {
      "title": "REST API示例",
      "category": "文档"
    }
  }'

# 批量添加向量
curl -X POST http://localhost:6333/vectors/batch \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer grape-api-key-admin" \
  -d '{
    "vectors": [
      {
        "id": "batch_1",
        "vector": [0.1, 0.2, 0.3],
        "payload": {"title": "批量文档1"}
      },
      {
        "id": "batch_2", 
        "vector": [0.4, 0.5, 0.6],
        "payload": {"title": "批量文档2"}
      }
    ]
  }'

# 搜索向量
curl -X POST http://localhost:6333/vectors/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer grape-api-key-admin" \
  -d '{
    "vector": [0.1, 0.2, 0.3],
    "limit": 10,
    "filter": {
      "conditions": [
        {
          "field": "category",
          "operator": "eq",
          "value": "文档"
        }
      ]
    }
  }'

# 获取向量
curl http://localhost:6333/vectors/rest_doc_1 \
  -H "Authorization: Bearer grape-api-key-admin"

# 删除向量
curl -X DELETE http://localhost:6333/vectors/rest_doc_1 \
  -H "Authorization: Bearer grape-api-key-admin"

# 获取统计信息
curl http://localhost:6333/stats \
  -H "Authorization: Bearer grape-api-key-admin"
```

#### JavaScript客户端

```javascript
class GrapeVectorDbClient {
    constructor(baseUrl, apiKey) {
        this.baseUrl = baseUrl;
        this.apiKey = apiKey;
    }

    async request(method, path, data = null) {
        const options = {
            method,
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${this.apiKey}`
            }
        };

        if (data) {
            options.body = JSON.stringify(data);
        }

        const response = await fetch(`${this.baseUrl}${path}`, options);
        
        if (!response.ok) {
            throw new Error(`HTTP error! status: ${response.status}`);
        }

        return response.json();
    }

    async addVector(id, vector, payload = {}) {
        return this.request('POST', '/vectors', {
            id, vector, payload
        });
    }

    async searchVectors(vector, limit = 10, filter = null) {
        return this.request('POST', '/vectors/search', {
            vector, limit, filter
        });
    }

    async getVector(id) {
        return this.request('GET', `/vectors/${id}`);
    }

    async deleteVector(id) {
        return this.request('DELETE', `/vectors/${id}`);
    }

    async getStats() {
        return this.request('GET', '/stats');
    }
}

// 使用示例
const client = new GrapeVectorDbClient('http://localhost:6333', 'grape-api-key-admin');

// 添加向量
await client.addVector('js_doc_1', [0.1, 0.2, 0.3], {
    title: 'JavaScript示例',
    language: 'zh'
});

// 搜索向量
const results = await client.searchVectors([0.1, 0.2, 0.3], 5);
console.log('搜索结果:', results);
```

## 🐳 Docker 部署

### Dockerfile

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin grape-vector-db-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/grape-vector-db-server /usr/local/bin/
COPY --from=builder /app/config.toml /app/config.toml

WORKDIR /app

EXPOSE 6333 6334 9090

CMD ["grape-vector-db-server", "--config", "config.toml"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  grape-vector-db:
    build: .
    ports:
      - "6333:6333"   # REST API
      - "6334:6334"   # gRPC API
      - "9090:9090"   # Prometheus指标
    volumes:
      - ./data:/app/data
      - ./config.toml:/app/config.toml
    environment:
      - GRAPE_LOG_LEVEL=info
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:6333/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    restart: unless-stopped

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9091:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana-storage:/var/lib/grafana

volumes:
  grafana-storage:
```

### Prometheus配置

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'grape-vector-db'
    static_configs:
      - targets: ['grape-vector-db:9090']
    metrics_path: /metrics
    scrape_interval: 10s
```

## 📊 监控和运维

### 健康检查

```bash
# 基本健康检查
curl http://localhost:6333/health

# 详细健康检查
curl http://localhost:6333/health/detailed

# 响应示例
{
  "status": "healthy",
  "timestamp": "2024-01-01T10:00:00Z",
  "uptime_seconds": 3600,
  "version": "1.0.0",
  "components": {
    "storage": "healthy",
    "index": "healthy",
    "cache": "healthy"
  },
  "metrics": {
    "memory_usage_mb": 512.5,
    "cpu_usage_percent": 15.2,
    "disk_usage_mb": 1024.0,
    "active_connections": 5
  }
}
```

### Prometheus指标

```bash
# 获取所有指标
curl http://localhost:9090/metrics

# 关键指标说明
grape_vectors_total           # 总向量数量
grape_queries_total           # 总查询次数
grape_query_duration_seconds  # 查询延迟分布
grape_memory_usage_bytes      # 内存使用量
grape_disk_usage_bytes        # 磁盘使用量
grape_cache_hit_ratio         # 缓存命中率
grape_active_connections      # 活跃连接数
```

### 日志管理

```bash
# 实时查看日志
docker logs -f grape-vector-db

# 结构化日志输出示例
{
  "timestamp": "2024-01-01T10:00:00Z",
  "level": "INFO",
  "target": "grape_vector_db::server",
  "message": "gRPC server started on port 6334",
  "fields": {
    "port": 6334,
    "protocol": "grpc"
  }
}
```

## 🔧 性能调优

### 内存优化

```toml
[storage]
# 减少缓存大小
cache_size_mb = 256
# 减少写缓冲
write_buffer_size_mb = 32
# 启用压缩
compression_enabled = true

[index]
# 降低HNSW参数
m = 16
ef_construction = 200
ef_search = 100
```

### 高性能配置

```toml
[storage]
# 增大缓存
cache_size_mb = 1024
# 增大写缓冲
write_buffer_size_mb = 128
# 关闭压缩（CPU换存储）
compression_enabled = false

[index]
# 提高HNSW参数
m = 48
ef_construction = 800
ef_search = 400

[server]
# 增加工作线程
worker_threads = 16
# 增加连接数
max_connections = 2000
```

### 负载均衡

```nginx
# nginx.conf
upstream grape_vector_db {
    server grape-node-1:6333;
    server grape-node-2:6333;
    server grape-node-3:6333;
}

server {
    listen 80;
    
    location / {
        proxy_pass http://grape_vector_db;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

## 🛠️ 故障排除

### 常见问题

1. **服务无法启动**
   ```
   错误: Failed to bind to port 6333
   解决: 检查端口是否被占用: lsof -i :6333
   ```

2. **内存不足**
   ```
   错误: Out of memory during index building
   解决: 增加cache_size_mb或减少batch_size
   ```

3. **连接超时**
   ```
   错误: Connection timeout
   解决: 增加request_timeout_secs或检查网络连接
   ```

### 调试模式

```bash
# 启用详细日志
export GRAPE_LOG_LEVEL=debug

# 启用性能分析
export GRAPE_PROFILE_ENABLED=true

# 启动服务
./grape-vector-db-server --config config.toml
```

## 📈 性能基准

### 测试环境
- CPU: Intel Xeon E5-2686 v4 (8核)
- 内存: 32GB DDR4
- 存储: NVMe SSD
- 网络: 1Gbps

### 基准结果

| 操作类型 | QPS | 延迟 (P95) | CPU使用 | 内存使用 |
|---------|-----|-----------|---------|----------|
| 向量插入 | 15,000+ | 8ms | 60% | 1GB |
| 向量搜索 | 25,000+ | 5ms | 40% | 512MB |
| 批量插入 | 30,000+ | 15ms | 80% | 2GB |
| 过滤搜索 | 12,000+ | 12ms | 50% | 512MB |

## 🔗 相关链接

- [内嵌模式部署指南](./deployment-embedded-mode.md)
- [3节点集群部署指南](./deployment-cluster-3node.md)
- [API 参考文档](./api-reference.md)
- [配置参考](./configuration-reference.md)
- [性能调优指南](./performance-tuning.md)