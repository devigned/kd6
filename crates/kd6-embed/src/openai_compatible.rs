use std::time::Duration;

use async_trait::async_trait;

use kd6_core::embedding::EmbeddingProvider;
use kd6_core::OmsError;

/// Request timeout for embedding API calls (30 seconds).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Connection timeout for embedding API calls (10 seconds).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Embedding provider that calls any OpenAI-compatible `/v1/embeddings` endpoint.
///
/// Works with OpenAI, Azure OpenAI, Ollama, vLLM, LiteLLM, and any
/// service that speaks the same protocol.
pub struct OpenAiCompatibleEmbedder {
    client: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    dimensions: usize,
}

#[derive(serde::Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(serde::Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(serde::Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
    index: usize,
}

impl OpenAiCompatibleEmbedder {
    /// Create a new OpenAI-compatible embedding provider.
    ///
    /// - `endpoint`: Base URL (e.g., `https://api.openai.com/v1`)
    /// - `model`: Model name (e.g., `text-embedding-3-small`)
    /// - `api_key`: Optional API key (not needed for local providers like Ollama)
    /// - `dimensions`: Expected embedding dimensionality
    pub fn new(
        endpoint: String,
        model: String,
        api_key: Option<String>,
        dimensions: usize,
    ) -> Result<Self, OmsError> {
        tracing::info!(
            endpoint = %endpoint,
            model = %model,
            dimensions,
            "configured OpenAI-compatible embedding provider"
        );

        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| OmsError::Internal(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model,
            api_key,
            dimensions,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatibleEmbedder {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmsError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let url = format!("{}/embeddings", self.endpoint);

        let body = EmbedRequest {
            model: &self.model,
            input: texts,
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let response = req.send().await.map_err(|e| {
            OmsError::Internal(format!(
                "embedding request to {} failed: {e}",
                self.endpoint
            ))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".into());
            return Err(if status.as_u16() == 429 {
                OmsError::Internal(format!("embedding endpoint rate limited (429): {body}"))
            } else if status.is_client_error() {
                OmsError::InvalidInput(format!(
                    "embedding endpoint rejected request ({status}): {body}"
                ))
            } else {
                OmsError::Internal(format!("embedding endpoint returned {status}: {body}"))
            });
        }

        let embed_response: EmbedResponse = response
            .json()
            .await
            .map_err(|e| OmsError::Internal(format!("failed to parse embedding response: {e}")))?;

        if embed_response.data.len() != texts.len() {
            return Err(OmsError::Internal(format!(
                "embedding endpoint returned {} vectors for {} inputs",
                embed_response.data.len(),
                texts.len()
            )));
        }

        // Sort by index to maintain input order (providers may return out-of-order)
        let mut sorted = embed_response.data;
        sorted.sort_by_key(|d| d.index);

        // Validate index set is exactly 0..N (no duplicates, gaps, or out-of-range)
        for (i, item) in sorted.iter().enumerate() {
            if item.index != i {
                return Err(OmsError::Internal(format!(
                    "embedding response has invalid index sequence: expected {i}, got {}",
                    item.index
                )));
            }
        }

        // Validate dimensionality of all results
        for (i, item) in sorted.iter().enumerate() {
            if item.embedding.len() != self.dimensions {
                return Err(OmsError::Internal(format!(
                    "embedding at index {i} has {} dimensions, expected {}",
                    item.embedding.len(),
                    self.dimensions
                )));
            }
        }

        Ok(sorted.into_iter().map(|d| d.embedding).collect())
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, OmsError> {
        let results = self.embed_texts(&[query.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| OmsError::Internal("embedding returned no results".into()))
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}
