use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;
use tokio::sync::Mutex;

use kd6_core::embedding::EmbeddingProvider;
use kd6_core::OmsError;

/// In-process embedding provider using fastembed (ONNX Runtime).
///
/// Downloads the model on first use (~25MB for the default model)
/// and caches it locally. No external services or API keys required.
pub struct LocalEmbedder {
    model: Arc<Mutex<TextEmbedding>>,
    model_id: String,
    dimensions: usize,
}

impl LocalEmbedder {
    /// Create a new local embedder with the default model (all-MiniLM-L6-v2, 384 dimensions).
    pub fn new() -> Result<Self, OmsError> {
        Self::with_model(EmbeddingModel::AllMiniLML6V2)
    }

    /// Create a local embedder with a specific fastembed model.
    pub fn with_model(model: EmbeddingModel) -> Result<Self, OmsError> {
        let info = TextEmbedding::get_model_info(&model)
            .map_err(|e| OmsError::Internal(format!("unknown embedding model: {e}")))?;
        let model_id = info.model_code.clone();
        let dimensions = info.dim;

        tracing::info!(
            model = %model_id,
            dimensions,
            "initializing local embedding model"
        );

        let embedder =
            TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(true))
                .map_err(|e| {
                    OmsError::Internal(format!("failed to initialize embedding model: {e}"))
                })?;

        Ok(Self {
            model: Arc::new(Mutex::new(embedder)),
            model_id,
            dimensions,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbedder {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmsError> {
        let texts = texts.to_vec();
        let model = Arc::clone(&self.model);

        // fastembed is synchronous; run on a blocking thread to avoid
        // starving the async runtime.
        tokio::task::spawn_blocking(move || {
            let model = model.blocking_lock();
            let str_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            model
                .embed(str_refs, None)
                .map_err(|e| OmsError::Internal(format!("embedding failed: {e}")))
        })
        .await
        .map_err(|e| OmsError::Internal(format!("embedding task panicked: {e}")))?
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
        &self.model_id
    }
}
