# 🍇 Grape Vector Database Documentation

Welcome to the Grape Vector Database documentation! This documentation is available in multiple languages.

## Language Selection / 语言选择

### English Documentation
For English documentation, please visit: [docs/en/](./en/)

### 中文文档  
中文文档请访问：[docs/zh/](./zh/)

---

## Quick Links / 快速链接

### Deployment Guides / 部署指南

| Deployment Mode | English | 中文 |
|----------------|---------|------|
| **🔧 Embedded Mode** | [EN](./en/deployment-embedded-mode.md) | [中文](./zh/deployment-embedded-mode.md) |
| **🖥️ Single Node** | [EN](./en/deployment-single-node.md) | [中文](./zh/deployment-single-node.md) |
| **🏭 3-Node Cluster** | [EN](./en/deployment-cluster-3node.md) | [中文](./zh/deployment-cluster-3node.md) |

### Examples / 示例代码

| Example Type | File | Description |
|-------------|------|-------------|
| **Embedded Mode** | `embedded_mode_simple.rs` | Simple embedded usage / 简单内嵌模式使用 |
| **Single Node** | `single_node_simple.rs` | Single node server / 单节点服务器 |
| **3-Node Cluster** | `cluster_3node_simple.rs` | 3-node cluster demo / 3节点集群演示 |

### Running Examples / 运行示例

```bash
# Embedded mode example / 内嵌模式示例
cargo run --example embedded_mode_simple

# Single node server example / 单节点服务器示例  
cargo run --example single_node_simple

# 3-node cluster example / 3节点集群示例
cargo run --example cluster_3node_simple
```

---

## Documentation Structure / 文档结构

```
docs/
├── README.md                    # This file / 本文件
├── en/                         # English documentation / 英文文档
│   ├── deployment-embedded-mode.md
│   ├── deployment-single-node.md
│   └── deployment-cluster-3node.md
├── zh/                         # Chinese documentation / 中文文档
│   ├── deployment-embedded-mode.md
│   ├── deployment-single-node.md
│   └── deployment-cluster-3node.md
└── [other files...]           # Other documentation / 其他文档
```

---

## Contributing / 贡献

When updating documentation, please maintain consistency between language versions.

更新文档时，请保持各语言版本之间的一致性。

---

## Support / 支持

- GitHub Issues: [Create an issue](https://github.com/putao520/grape-vector-db/issues)
- Email: Contact the maintainers / 联系维护者