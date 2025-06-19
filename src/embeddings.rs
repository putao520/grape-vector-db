use crate::{
    config::{EmbeddingConfig, VectorDbConfig},
    errors::{Result, VectorDbError},
};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 嵌入提供者trait
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>>;
    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn embedding_dimension(&self) -> usize;
}

/// OpenAI兼容的嵌入请求
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: serde_json::Value,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding_format: Option<String>,
}

/// OpenAI兼容的嵌入响应
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    model: String,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
    object: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Usage {
    prompt_tokens: u32,
    total_tokens: u32,
}

/// OpenAI兼容的嵌入提供商
pub struct OpenAICompatibleProvider {
    client: Client,
    config: EmbeddingConfig,
    endpoint: String,
}

impl OpenAICompatibleProvider {
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        // 构建HTTP客户端
        let mut headers = HeaderMap::new();

        // 添加认证头
        if let Some(api_key) = &config.api_key {
            let auth_value = format!("Bearer {}", api_key);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth_value).map_err(|e| {
                    VectorDbError::embedding_error(format!("无效的API密钥格式: {}", e))
                })?,
            );
        }

        // 添加内容类型
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // 添加自定义头
        for (key, value) in &config.headers {
            let header_name = key.parse::<reqwest::header::HeaderName>().map_err(|e| {
                VectorDbError::embedding_error(format!("无效的请求头名称 {}: {}", key, e))
            })?;
            let header_value = HeaderValue::from_str(value).map_err(|e| {
                VectorDbError::embedding_error(format!("无效的请求头值 {}: {}", value, e))
            })?;
            headers.insert(header_name, header_value);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .default_headers(headers)
            .build()
            .map_err(|e| VectorDbError::embedding_error(format!("无法创建HTTP客户端: {}", e)))?;

        // 确定端点URL
        let endpoint = match config.endpoint.clone() {
            Some(url) => url,
            None => {
                match config.provider.as_str() {
                    "openai" => "https://api.openai.com/v1/embeddings".to_string(),
                    "azure" => {
                        return Err(VectorDbError::ConfigError(
                            "Azure提供商需要指定endpoint".to_string(),
                        ));
                    }
                    _ => "http://localhost:11434/api/embeddings".to_string(), // 默认本地端点
                }
            }
        };

        Ok(Self {
            client,
            config,
            endpoint,
        })
    }

    async fn request_embeddings(&self, input: serde_json::Value) -> Result<EmbeddingResponse> {
        let request = EmbeddingRequest {
            input,
            model: self.config.model.clone(),
            encoding_format: Some("float".to_string()),
        };

        let mut last_error = None;

        // 重试逻辑
        for attempt in 0..=self.config.retry_attempts {
            match self.client.post(&self.endpoint).json(&request).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        return response.json().await.map_err(|e| {
                            VectorDbError::embedding_error(format!("解析响应失败: {}", e))
                        });
                    } else {
                        let status = response.status();
                        let text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "未知错误".to_string());
                        last_error = Some(VectorDbError::embedding_error(format!(
                            "API请求失败 (状态码: {}): {}",
                            status, text
                        )));
                    }
                }
                Err(e) => {
                    last_error = Some(VectorDbError::embedding_error(format!(
                        "网络请求失败: {}",
                        e
                    )));
                }
            }

            if attempt < self.config.retry_attempts {
                tokio::time::sleep(Duration::from_millis(1000 * (attempt + 1) as u64)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| VectorDbError::embedding_error("未知错误".to_string())))
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAICompatibleProvider {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let response = self
            .request_embeddings(serde_json::Value::String(text.to_string()))
            .await?;

        if response.data.is_empty() {
            return Err(VectorDbError::embedding_error(
                "响应中没有嵌入数据".to_string(),
            ));
        }

        Ok(response.data[0].embedding.clone())
    }

    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // 分批处理
        let mut all_embeddings = Vec::new();

        for chunk in texts.chunks(self.config.batch_size) {
            let input = if chunk.len() == 1 {
                serde_json::Value::String(chunk[0].clone())
            } else {
                serde_json::Value::Array(
                    chunk
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                )
            };

            let response = self.request_embeddings(input).await?;

            let mut batch_embeddings: Vec<_> = response
                .data
                .into_iter()
                .map(|data| data.embedding)
                .collect();

            all_embeddings.append(&mut batch_embeddings);
        }

        Ok(all_embeddings)
    }

    fn embedding_dimension(&self) -> usize {
        self.config.dimension.unwrap_or(1536) // 默认OpenAI维度
    }
}

/// 模拟嵌入提供者（用于测试）
pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // 模拟嵌入生成：基于文本内容生成确定性向量
        let mut vector = vec![0.0; self.dimension];
        let bytes = text.as_bytes();

        for (i, val) in vector.iter_mut().enumerate() {
            let idx = i % bytes.len();
            *val = (bytes[idx] as f32 / 255.0 + i as f32 * 0.01) % 1.0 - 0.5;
        }

        // 归一化向量
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in vector.iter_mut() {
                *val /= norm;
            }
        }

        Ok(vector)
    }

    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::new();
        for text in texts {
            embeddings.push(self.generate_embedding(text).await?);
        }
        Ok(embeddings)
    }

    fn embedding_dimension(&self) -> usize {
        self.dimension
    }
}

/// 创建嵌入提供者
pub fn create_provider(config: &VectorDbConfig) -> Result<Box<dyn EmbeddingProvider>> {
    match config.embedding.provider.as_str() {
        "openai" | "azure" | "nvidia" | "huggingface" | "ollama" => Ok(Box::new(
            OpenAICompatibleProvider::new(config.embedding.clone())?,
        )),
        "mock" => {
            let dimension = config
                .embedding
                .dimension
                .unwrap_or(config.vector_dimension);
            Ok(Box::new(MockEmbeddingProvider::new(dimension)))
        }
        provider => Err(VectorDbError::ConfigError(format!(
            "不支持的嵌入提供商: {}",
            provider
        ))),
    }
}

/// 便捷函数：使用自定义配置创建OpenAI兼容提供商
pub fn create_openai_compatible_provider(
    endpoint: String,
    api_key: String,
    model: String,
) -> Result<Box<dyn EmbeddingProvider>> {
    let config = EmbeddingConfig {
        provider: "openai".to_string(),
        endpoint: Some(endpoint),
        api_key: Some(api_key),
        model,
        ..Default::default()
    };

    Ok(Box::new(OpenAICompatibleProvider::new(config)?))
}
