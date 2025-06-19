# 🍇 Grape Vector Database - Single Node Deployment Guide

## 📖 Overview

Single node deployment provides a standalone vector database server with full gRPC and REST API support. This mode is ideal for:

- **Microservices Architecture**: Independent vector database service
- **API Services**: HTTP-based vector search endpoints
- **Development & Testing**: Full-featured development environment
- **Medium-scale Applications**: 10K-100K documents with moderate query load
- **Multi-client Applications**: Shared database across multiple applications

## ✨ Key Features

- **🌐 Dual Protocol Support**: Both gRPC and REST API
- **🐳 Docker Ready**: Complete containerization support
- **📊 Built-in Monitoring**: Prometheus metrics and health checks
- **🔐 Authentication**: API key authentication and RBAC support
- **⚡ High Performance**: 8K+ write QPS, 25K+ read QPS
- **🛡️ Production Ready**: Automatic recovery, logging, and monitoring
- **🔄 Load Balancing**: Support for multiple client connections

## 🚀 Quick Start

### Basic Server Setup

```rust
use grape_vector_db::*;
use std::time::Duration;
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Server configuration
    let config = ServerConfig {
        listen_address: "0.0.0.0:8080".to_string(),
        data_dir: "./vector_db_data".to_string(),
        log_level: "info".to_string(),
        enable_cors: true,
        enable_metrics: true,
        max_connections: 1000,
    };
    
    // Start server
    let mut server = VectorDbServer::new(config).await?;
    server.start().await?;
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    server.shutdown().await?;
    
    Ok(())
}
```

### Docker Deployment

#### Dockerfile

```dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/grape-vector-db /usr/local/bin/
COPY --from=builder /app/config/ /etc/grape-vector-db/

EXPOSE 8080 9090

CMD ["grape-vector-db", "--config", "/etc/grape-vector-db/server.yaml"]
```

#### Docker Compose

```yaml
version: '3.8'

services:
  grape-vector-db:
    build: .
    ports:
      - "8080:8080"   # API port
      - "9090:9090"   # Metrics port
    volumes:
      - vector_data:/var/lib/grape-vector-db
      - ./config:/etc/grape-vector-db
    environment:
      - RUST_LOG=info
      - GRAPE_DB_DATA_DIR=/var/lib/grape-vector-db
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  # Optional: Prometheus monitoring
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9091:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
    depends_on:
      - grape-vector-db

  # Optional: Grafana dashboard
  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana_data:/var/lib/grafana
    depends_on:
      - prometheus

volumes:
  vector_data:
  grafana_data:
```

### Configuration File

Create `config/server.yaml`:

```yaml
# Server configuration
server:
  listen_address: "0.0.0.0:8080"
  grpc_port: 9000
  metrics_port: 9090
  max_connections: 1000
  request_timeout_ms: 30000
  
# Database configuration
database:
  data_dir: "/var/lib/grape-vector-db"
  vector_dimension: 1536
  max_documents: 1000000
  cache_size_mb: 512
  enable_compression: true
  backup_interval_hours: 24

# Index configuration
index:
  type: "hnsw"
  hnsw_m: 16
  hnsw_ef_construction: 200
  hnsw_ef_search: 64
  enable_binary_quantization: true

# Security configuration
security:
  enable_auth: true
  api_keys_file: "/etc/grape-vector-db/api_keys.yaml"
  enable_tls: false
  cert_file: ""
  key_file: ""

# Monitoring configuration
monitoring:
  enable_metrics: true
  enable_tracing: true
  log_level: "info"
  metrics_path: "/metrics"
  health_check_path: "/health"

# Performance configuration
performance:
  num_threads: 4
  max_batch_size: 1000
  query_timeout_ms: 5000
  enable_query_cache: true
  cache_ttl_seconds: 300
```

## 📡 API Reference

### REST API

#### Health Check

```bash
# Check server health
curl http://localhost:8080/health

# Response
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "database_status": "ready",
  "memory_usage_mb": 256
}
```

#### Document Operations

```bash
# Add document
curl -X POST http://localhost:8080/api/v1/documents \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "id": "doc_1",
    "content": "Sample document content",
    "metadata": {
      "category": "technology",
      "author": "John Doe"
    }
  }'

# Get document
curl http://localhost:8080/api/v1/documents/doc_1 \
  -H "X-API-Key: your-api-key"

# Search documents
curl -X POST http://localhost:8080/api/v1/search \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "query": "machine learning",
    "top_k": 10,
    "filter": {
      "category": "technology"
    }
  }'
```

#### Vector Operations

```bash
# Add vector
curl -X POST http://localhost:8080/api/v1/vectors \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "id": "vec_1",
    "vector": [0.1, 0.2, 0.3, ...],
    "payload": {
      "title": "Sample Vector",
      "category": "tech"
    }
  }'

# Vector search
curl -X POST http://localhost:8080/api/v1/vectors/search \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "vector": [0.1, 0.2, 0.3, ...],
    "top_k": 5,
    "filter": {
      "category": "tech"
    }
  }'
```

#### Batch Operations

```bash
# Batch add documents
curl -X POST http://localhost:8080/api/v1/documents/batch \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "documents": [
      {
        "id": "doc_1",
        "content": "First document",
        "metadata": {"category": "tech"}
      },
      {
        "id": "doc_2", 
        "content": "Second document",
        "metadata": {"category": "science"}
      }
    ]
  }'
```

### gRPC API

#### Proto Definition

```protobuf
syntax = "proto3";

package grape.vector.v1;

service VectorService {
  rpc AddDocument(AddDocumentRequest) returns (AddDocumentResponse);
  rpc GetDocument(GetDocumentRequest) returns (GetDocumentResponse);
  rpc SearchDocuments(SearchRequest) returns (SearchResponse);
  rpc AddVector(AddVectorRequest) returns (AddVectorResponse);
  rpc SearchVectors(VectorSearchRequest) returns (VectorSearchResponse);
  rpc GetHealth(HealthRequest) returns (HealthResponse);
}

message Document {
  string id = 1;
  string content = 2;
  map<string, string> metadata = 3;
  repeated float vector = 4;
}

message SearchRequest {
  string query = 1;
  int32 top_k = 2;
  map<string, string> filter = 3;
}

message SearchResponse {
  repeated SearchResult results = 1;
}

message SearchResult {
  Document document = 1;
  float score = 2;
}
```

#### Client Example

```rust
use tonic::Request;
use grape_vector_db_proto::vector_service_client::VectorServiceClient;
use grape_vector_db_proto::{AddDocumentRequest, Document};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let mut client = VectorServiceClient::connect("http://localhost:9000").await?;
    
    // Add document
    let request = Request::new(AddDocumentRequest {
        document: Some(Document {
            id: "doc_1".to_string(),
            content: "Sample document".to_string(),
            metadata: [("category".to_string(), "tech".to_string())].into(),
            vector: vec![],
        }),
    });
    
    let response = client.add_document(request).await?;
    println!("Document added: {}", response.get_ref().document_id);
    
    Ok(())
}
```

## 🔐 Security Configuration

### API Key Authentication

Create `config/api_keys.yaml`:

```yaml
api_keys:
  - key: "sk-prod-abc123def456"
    name: "Production API"
    permissions:
      - "read"
      - "write"
      - "admin"
    rate_limit:
      requests_per_minute: 1000
    expires_at: "2024-12-31T23:59:59Z"
    
  - key: "sk-readonly-xyz789"
    name: "Read-only API"
    permissions:
      - "read"
    rate_limit:
      requests_per_minute: 500
    expires_at: null

  - key: "sk-dev-test123"
    name: "Development API"
    permissions:
      - "read"
      - "write"
    rate_limit:
      requests_per_minute: 100
    expires_at: "2024-06-30T23:59:59Z"
```

### TLS Configuration

```yaml
# In server.yaml
security:
  enable_tls: true
  cert_file: "/etc/tls/server.crt"
  key_file: "/etc/tls/server.key"
  ca_file: "/etc/tls/ca.crt"
  client_auth: "require"
```

Generate certificates:

```bash
# Generate CA
openssl genrsa -out ca.key 4096
openssl req -new -x509 -key ca.key -sha256 -subj "/C=US/ST=CA/O=MyOrg/CN=MyCA" -days 3650 -out ca.crt

# Generate server certificate
openssl genrsa -out server.key 4096
openssl req -new -key server.key -out server.csr -subj "/C=US/ST=CA/O=MyOrg/CN=localhost"
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out server.crt -days 365 -sha256
```

## 📊 Monitoring and Observability

### Prometheus Metrics

Default metrics exposed at `/metrics`:

```
# Database metrics
grape_db_documents_total{collection="default"} 10000
grape_db_vectors_total{collection="default"} 10000
grape_db_index_size_bytes{collection="default"} 52428800

# Performance metrics
grape_db_search_duration_seconds{quantile="0.5"} 0.005
grape_db_search_duration_seconds{quantile="0.95"} 0.015
grape_db_search_duration_seconds{quantile="0.99"} 0.025

# Cache metrics
grape_db_cache_hits_total{cache_type="vector"} 8500
grape_db_cache_misses_total{cache_type="vector"} 1500
grape_db_cache_hit_ratio{cache_type="vector"} 0.85

# HTTP metrics
grape_http_requests_total{method="POST",endpoint="/api/v1/search",status="200"} 5000
grape_http_request_duration_seconds{method="POST",endpoint="/api/v1/search",quantile="0.95"} 0.012
```

### Grafana Dashboard

Sample dashboard configuration:

```json
{
  "dashboard": {
    "title": "Grape Vector DB",
    "panels": [
      {
        "title": "Request Rate",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(grape_http_requests_total[5m])",
            "legendFormat": "{{method}} {{endpoint}}"
          }
        ]
      },
      {
        "title": "Search Latency",
        "type": "graph", 
        "targets": [
          {
            "expr": "grape_db_search_duration_seconds{quantile=\"0.95\"}",
            "legendFormat": "95th percentile"
          }
        ]
      },
      {
        "title": "Cache Hit Ratio",
        "type": "singlestat",
        "targets": [
          {
            "expr": "grape_db_cache_hit_ratio{cache_type=\"vector\"}",
            "legendFormat": "Vector Cache"
          }
        ]
      }
    ]
  }
}
```

### Health Checks

#### Kubernetes Health Checks

```yaml
apiVersion: v1
kind: Pod
spec:
  containers:
  - name: grape-vector-db
    image: grape-vector-db:latest
    ports:
    - containerPort: 8080
    - containerPort: 9090
    livenessProbe:
      httpGet:
        path: /health
        port: 8080
      initialDelaySeconds: 30
      periodSeconds: 10
      timeoutSeconds: 5
      failureThreshold: 3
    readinessProbe:
      httpGet:
        path: /ready
        port: 8080
      initialDelaySeconds: 5
      periodSeconds: 5
      timeoutSeconds: 3
      failureThreshold: 2
```

#### Custom Health Checks

```rust
// Custom health check implementation
#[derive(Serialize)]
struct HealthStatus {
    status: String,
    timestamp: String,
    version: String,
    database: DatabaseHealth,
    memory: MemoryHealth,
    disk: DiskHealth,
}

#[derive(Serialize)]
struct DatabaseHealth {
    status: String,
    document_count: u64,
    index_status: String,
    last_backup: Option<String>,
}

async fn health_check(db: &VectorDatabase) -> HealthStatus {
    let stats = db.get_stats().await;
    let memory_info = get_memory_info().await;
    let disk_info = get_disk_info().await;
    
    HealthStatus {
        status: if stats.is_healthy() { "healthy" } else { "unhealthy" },
        timestamp: chrono::Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: DatabaseHealth {
            status: if stats.document_count > 0 { "ready" } else { "empty" },
            document_count: stats.document_count,
            index_status: "ready".to_string(),
            last_backup: None,
        },
        memory: MemoryHealth {
            used_mb: memory_info.used_mb,
            available_mb: memory_info.available_mb,
            usage_percent: memory_info.usage_percent,
        },
        disk: DiskHealth {
            used_gb: disk_info.used_gb,
            available_gb: disk_info.available_gb,
            usage_percent: disk_info.usage_percent,
        },
    }
}
```

## ⚡ Performance Tuning

### Hardware Recommendations

#### Minimum Requirements
- **CPU**: 2 cores, 2.0GHz
- **RAM**: 4GB
- **Storage**: 20GB SSD
- **Network**: 100Mbps

#### Recommended Configuration
- **CPU**: 4-8 cores, 3.0GHz+
- **RAM**: 16-32GB
- **Storage**: 100GB+ NVMe SSD
- **Network**: 1Gbps+

#### High-Performance Setup
- **CPU**: 16+ cores, 3.5GHz+
- **RAM**: 64GB+
- **Storage**: 500GB+ NVMe SSD (RAID 1)
- **Network**: 10Gbps+

### Performance Benchmarks

| Configuration | Write QPS | Read QPS | Search QPS | Memory Usage |
|---------------|-----------|----------|------------|--------------|
| **Minimal** | 2,000 | 8,000 | 1,500 | 2GB |
| **Standard** | 8,000 | 25,000 | 5,000 | 8GB |
| **High-Perf** | 15,000 | 50,000 | 12,000 | 32GB |

### Optimization Configuration

```yaml
# High-performance configuration
performance:
  # CPU optimization
  num_threads: 16
  worker_threads: 8
  blocking_threads: 4
  
  # Memory optimization
  cache_size_mb: 8192
  vector_cache_mb: 4096
  metadata_cache_mb: 2048
  query_cache_mb: 2048
  
  # I/O optimization
  max_batch_size: 5000
  batch_timeout_ms: 100
  write_buffer_size_mb: 256
  read_buffer_size_mb: 128
  
  # Index optimization
  hnsw_ef_search: 128
  enable_binary_quantization: true
  enable_parallel_search: true
  search_timeout_ms: 1000
  
  # Network optimization
  max_connections: 5000
  connection_timeout_ms: 30000
  keepalive_interval_ms: 60000
  tcp_nodelay: true
```

## 🚀 Load Balancing

### Nginx Configuration

```nginx
upstream grape_vector_db {
    least_conn;
    server 10.0.1.10:8080 max_fails=3 fail_timeout=30s;
    server 10.0.1.11:8080 max_fails=3 fail_timeout=30s;
    server 10.0.1.12:8080 max_fails=3 fail_timeout=30s;
}

server {
    listen 80;
    server_name vector-api.example.com;
    
    location /api/ {
        proxy_pass http://grape_vector_db;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Timeouts
        proxy_connect_timeout 5s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
        
        # Health checks
        proxy_next_upstream error timeout invalid_header http_500 http_502 http_503;
        proxy_next_upstream_tries 3;
    }
    
    location /health {
        access_log off;
        proxy_pass http://grape_vector_db;
    }
}
```

### HAProxy Configuration

```
global
    daemon
    maxconn 4096

defaults
    mode http
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms

frontend grape_frontend
    bind *:80
    default_backend grape_servers

backend grape_servers
    balance roundrobin
    option httpchk GET /health
    http-check expect status 200
    
    server grape1 10.0.1.10:8080 check inter 30s rise 2 fall 3
    server grape2 10.0.1.11:8080 check inter 30s rise 2 fall 3
    server grape3 10.0.1.12:8080 check inter 30s rise 2 fall 3
```

## 🐛 Troubleshooting

### Common Issues

#### High Memory Usage

```bash
# Check memory usage
curl http://localhost:8080/metrics | grep memory

# Solutions:
# 1. Reduce cache sizes
# 2. Enable compression
# 3. Use binary quantization
# 4. Increase available RAM
```

#### Slow Query Performance

```bash
# Check search metrics
curl http://localhost:8080/metrics | grep search_duration

# Solutions:
# 1. Increase HNSW ef_search parameter
# 2. Enable query caching
# 3. Use better hardware (SSD, more CPU)
# 4. Optimize vector dimensions
```

#### Connection Issues

```bash
# Check connection metrics  
curl http://localhost:8080/metrics | grep connection

# Solutions:
# 1. Increase max_connections
# 2. Check network connectivity
# 3. Verify API keys
# 4. Check firewall settings
```

### Debug Commands

```bash
# Enable debug logging
export RUST_LOG=debug
./grape-vector-db --config server.yaml

# Check server status
curl -v http://localhost:8080/health

# Monitor real-time metrics
watch -n 1 'curl -s http://localhost:8080/metrics | grep -E "(request|search|memory)"'

# Test API connectivity
curl -X POST http://localhost:8080/api/v1/search \
  -H "Content-Type: application/json" \
  -H "X-API-Key: test-key" \
  -d '{"query": "test", "top_k": 1}' \
  -v
```

## 📋 Production Checklist

### Pre-deployment

- [ ] Configure proper resource limits
- [ ] Set up monitoring and alerting
- [ ] Configure backup procedures
- [ ] Test API authentication
- [ ] Verify SSL/TLS certificates
- [ ] Configure log rotation
- [ ] Set up health checks
- [ ] Test failover procedures

### Post-deployment

- [ ] Monitor performance metrics
- [ ] Verify data persistence
- [ ] Test backup/restore procedures
- [ ] Monitor resource usage
- [ ] Set up alerting rules
- [ ] Document operational procedures
- [ ] Train operations team
- [ ] Schedule regular maintenance

---

This guide provides comprehensive coverage of single node deployment for Grape Vector Database. For smaller applications, see the [Embedded Mode](./deployment-embedded-mode.md) guide. For high-availability requirements, see the [3-Node Cluster](./deployment-cluster-3node.md) guide.