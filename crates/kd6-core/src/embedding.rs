use async_trait::async_trait;

use crate::error::OmsError;

/// Computes vector embeddings from text content.
///
/// Implementations may use a local model (e.g., ONNX via fastembed),
/// a remote API (e.g., OpenAI, Azure OpenAI, Ollama), or any
/// OpenAI-compatible embedding endpoint.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Compute embeddings for one or more text strings.
    ///
    /// Returns a vector of embedding vectors, one per input text.
    /// All vectors MUST have the same dimensionality.
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmsError>;

    /// Compute a single embedding for a search query.
    ///
    /// Some models use different prefixes for queries vs. documents
    /// (e.g., "query: " vs. "passage: " in E5/nomic models).
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, OmsError>;

    /// Return the dimensionality of embeddings produced by this provider.
    fn dimensions(&self) -> usize;

    /// Return a stable identifier for the embedding model.
    fn model_id(&self) -> &str;
}

/// A no-op embedding provider that never computes embeddings.
///
/// Used when no embedding provider is configured (pass-through mode).
/// Callers must supply their own embeddings for vector search;
/// keyword search remains available.
pub struct NoopEmbedder;

#[async_trait]
impl EmbeddingProvider for NoopEmbedder {
    async fn embed_texts(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, OmsError> {
        Err(OmsError::InvalidInput(
            "no embedding provider configured; supply embeddings in the request or \
             configure KD6_EMBEDDING_PROVIDER"
                .into(),
        ))
    }

    async fn embed_query(&self, _query: &str) -> Result<Vec<f32>, OmsError> {
        Err(OmsError::InvalidInput(
            "no embedding provider configured; supply embedding in the search request or \
             configure KD6_EMBEDDING_PROVIDER"
                .into(),
        ))
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn model_id(&self) -> &str {
        "none"
    }
}

/// Returns `true` if the provider is the no-op placeholder.
pub fn is_noop(provider: &dyn EmbeddingProvider) -> bool {
    provider.model_id() == "none" && provider.dimensions() == 0
}

// ---------------------------------------------------------------------------
// Automatic embedding helpers (OMS spec section 8.4)
//
// Shared by kd6-server and kd6-mcp so that both entry points apply the same
// server-side embedding logic.
// ---------------------------------------------------------------------------

/// Extract embeddable text from a JSON content value.
///
/// - String values are used directly.
/// - Object and array values have their string-typed leaves concatenated
///   (recursive traversal).
/// - Other types are serialized to their JSON representation.
pub fn content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            let mut parts = Vec::new();
            collect_string_leaves(content, &mut parts);
            if parts.is_empty() {
                serde_json::to_string(content).unwrap_or_default()
            } else {
                parts.join(" ")
            }
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Recursively collect string leaves from any JSON value.
fn collect_string_leaves(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_string_leaves(v, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                collect_string_leaves(v, out);
            }
        }
        _ => {}
    }
}

/// Compute an embedding for content if no embedding was provided and the
/// provider is active. Returns the embedding to use (either caller-provided
/// or freshly computed).
///
/// Implements OMS spec section 8.4.1 (write) behavior.
pub async fn auto_embed_content(
    provider: &dyn EmbeddingProvider,
    content: &serde_json::Value,
    existing_embedding: Option<Vec<f32>>,
) -> Result<Option<Vec<f32>>, OmsError> {
    if let Some(emb) = existing_embedding {
        // Validate dimensionality if provider is configured
        if !is_noop(provider) && emb.len() != provider.dimensions() {
            return Err(OmsError::InvalidInput(format!(
                "embedding has {} dimensions, expected {} for model {}",
                emb.len(),
                provider.dimensions(),
                provider.model_id()
            )));
        }
        return Ok(Some(emb));
    }

    if is_noop(provider) {
        return Ok(None);
    }

    let text = content_to_text(content);
    if text.is_empty() {
        return Ok(None);
    }

    let embeddings = provider.embed_texts(&[text]).await?;
    Ok(embeddings.into_iter().next())
}

/// Compute a query embedding if none was provided and the provider is active.
///
/// Implements OMS spec section 8.4.3 (search) behavior.
pub async fn auto_embed_query(
    provider: &dyn EmbeddingProvider,
    query: &str,
    existing_embedding: Option<Vec<f32>>,
) -> Result<Option<Vec<f32>>, OmsError> {
    if let Some(emb) = existing_embedding {
        if !is_noop(provider) && emb.len() != provider.dimensions() {
            return Err(OmsError::InvalidInput(format!(
                "query embedding has {} dimensions, expected {} for model {}",
                emb.len(),
                provider.dimensions(),
                provider.model_id()
            )));
        }
        return Ok(Some(emb));
    }

    if is_noop(provider) {
        return Ok(None);
    }

    if query.trim().is_empty() {
        return Ok(None);
    }

    let embedding = provider.embed_query(query).await?;
    Ok(Some(embedding))
}

/// Compute the embedding for a memory update request.
///
/// Implements OMS spec section 8.4.2 (update) behavior. Handles the
/// three-state `Option<Option<Vec<f32>>>` semantics:
///
/// - `None` — caller didn't mention embedding: auto-compute if content changed
/// - `Some(None)` — explicitly clear the embedding
/// - `Some(Some(v))` — explicitly set a new embedding (validated)
pub async fn auto_embed_update(
    provider: &dyn EmbeddingProvider,
    new_content: Option<&serde_json::Value>,
    embedding_field: Option<Option<Vec<f32>>>,
) -> Result<Option<Option<Vec<f32>>>, OmsError> {
    match embedding_field {
        // Caller explicitly provided a new embedding — validate and use it
        Some(Some(emb)) => {
            if !is_noop(provider) && emb.len() != provider.dimensions() {
                return Err(OmsError::InvalidInput(format!(
                    "embedding has {} dimensions, expected {} for model {}",
                    emb.len(),
                    provider.dimensions(),
                    provider.model_id()
                )));
            }
            Ok(Some(Some(emb)))
        }
        // Caller explicitly cleared the embedding — respect it
        Some(None) => Ok(Some(None)),
        // Caller didn't mention embedding — auto-compute if content changed
        None => {
            if let Some(content) = new_content {
                let computed = auto_embed_content(provider, content, None).await?;
                Ok(computed.map(Some))
            } else {
                // No content change, no embedding change — preserve existing
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- Deterministic fake embedder for tests ---

    struct FakeEmbedder;

    #[async_trait]
    impl EmbeddingProvider for FakeEmbedder {
        async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmsError> {
            Ok(texts
                .iter()
                .map(|t| vec![t.len() as f32, 0.0, 1.0])
                .collect())
        }
        async fn embed_query(&self, query: &str) -> Result<Vec<f32>, OmsError> {
            Ok(vec![query.len() as f32, 0.0, 1.0])
        }
        fn dimensions(&self) -> usize {
            3
        }
        fn model_id(&self) -> &str {
            "fake-3d"
        }
    }

    // --- content_to_text ---

    #[test]
    fn content_to_text_string() {
        assert_eq!(content_to_text(&json!("hello")), "hello");
    }

    #[test]
    fn content_to_text_object_extracts_string_leaves() {
        let val = json!({"text": "hello", "nested": {"msg": "world"}});
        let text = content_to_text(&val);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
    }

    #[test]
    fn content_to_text_array_extracts_string_leaves() {
        let val = json!(["alpha", {"inner": "beta"}, "gamma"]);
        let text = content_to_text(&val);
        assert!(text.contains("alpha"));
        assert!(text.contains("beta"));
        assert!(text.contains("gamma"));
    }

    #[test]
    fn content_to_text_number_serializes() {
        assert_eq!(content_to_text(&json!(42)), "42");
    }

    #[test]
    fn content_to_text_empty_object_falls_back_to_json() {
        let val = json!({"count": 5});
        let text = content_to_text(&val);
        // No string leaves, so falls back to JSON serialization
        assert!(text.contains("count"));
    }

    // --- is_noop ---

    #[test]
    fn noop_detected_correctly() {
        assert!(is_noop(&NoopEmbedder));
        assert!(!is_noop(&FakeEmbedder));
    }

    // --- auto_embed_content ---

    #[tokio::test]
    async fn auto_embed_content_uses_provided_embedding() {
        let result = auto_embed_content(&FakeEmbedder, &json!("text"), Some(vec![1.0, 2.0, 3.0]))
            .await
            .unwrap();
        assert_eq!(result, Some(vec![1.0, 2.0, 3.0]));
    }

    #[tokio::test]
    async fn auto_embed_content_rejects_wrong_dimensions() {
        let result = auto_embed_content(&FakeEmbedder, &json!("text"), Some(vec![1.0, 2.0])).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn auto_embed_content_computes_when_none_provided() {
        let result = auto_embed_content(&FakeEmbedder, &json!("hello"), None)
            .await
            .unwrap();
        // FakeEmbedder returns [len, 0.0, 1.0] — "hello" is 5 chars
        assert_eq!(result, Some(vec![5.0, 0.0, 1.0]));
    }

    #[tokio::test]
    async fn auto_embed_content_returns_none_for_noop() {
        let result = auto_embed_content(&NoopEmbedder, &json!("text"), None)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn auto_embed_content_noop_passes_through_provided() {
        let result = auto_embed_content(&NoopEmbedder, &json!("text"), Some(vec![1.0, 2.0]))
            .await
            .unwrap();
        assert_eq!(result, Some(vec![1.0, 2.0]));
    }

    #[tokio::test]
    async fn auto_embed_content_returns_none_for_empty_text() {
        let result = auto_embed_content(&FakeEmbedder, &json!(""), None)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    // --- auto_embed_query ---

    #[tokio::test]
    async fn auto_embed_query_computes_when_none_provided() {
        let result = auto_embed_query(&FakeEmbedder, "search term", None)
            .await
            .unwrap();
        assert_eq!(result, Some(vec![11.0, 0.0, 1.0]));
    }

    #[tokio::test]
    async fn auto_embed_query_uses_provided_embedding() {
        let result = auto_embed_query(&FakeEmbedder, "query", Some(vec![9.0, 8.0, 7.0]))
            .await
            .unwrap();
        assert_eq!(result, Some(vec![9.0, 8.0, 7.0]));
    }

    #[tokio::test]
    async fn auto_embed_query_returns_none_for_empty_query() {
        let result = auto_embed_query(&FakeEmbedder, "  ", None).await.unwrap();
        assert_eq!(result, None);
    }

    // --- auto_embed_update ---

    #[tokio::test]
    async fn auto_embed_update_explicit_embedding_accepted() {
        let result = auto_embed_update(
            &FakeEmbedder,
            Some(&json!("new content")),
            Some(Some(vec![1.0, 2.0, 3.0])),
        )
        .await
        .unwrap();
        assert_eq!(result, Some(Some(vec![1.0, 2.0, 3.0])));
    }

    #[tokio::test]
    async fn auto_embed_update_explicit_clear() {
        let result = auto_embed_update(&FakeEmbedder, Some(&json!("new")), Some(None))
            .await
            .unwrap();
        assert_eq!(result, Some(None));
    }

    #[tokio::test]
    async fn auto_embed_update_auto_computes_on_content_change() {
        let result = auto_embed_update(&FakeEmbedder, Some(&json!("new")), None)
            .await
            .unwrap();
        assert_eq!(result, Some(Some(vec![3.0, 0.0, 1.0])));
    }

    #[tokio::test]
    async fn auto_embed_update_preserves_existing_when_no_content_change() {
        let result = auto_embed_update(&FakeEmbedder, None, None).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn auto_embed_update_rejects_wrong_dimensions() {
        let result =
            auto_embed_update(&FakeEmbedder, Some(&json!("x")), Some(Some(vec![1.0]))).await;
        assert!(result.is_err());
    }
}
