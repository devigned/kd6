use kd6_core::embedding::EmbeddingProvider;
use kd6_embed::OpenAiCompatibleEmbedder;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to start a mock server and create an embedder pointing at it.
async fn mock_embedder(dims: usize) -> (MockServer, OpenAiCompatibleEmbedder) {
    let server = MockServer::start().await;
    let embedder =
        OpenAiCompatibleEmbedder::new(server.uri(), "test-model".into(), None, dims).unwrap();
    (server, embedder)
}

fn embedding_response(data: Vec<(usize, Vec<f32>)>) -> ResponseTemplate {
    let data: Vec<_> = data
        .into_iter()
        .map(|(idx, emb)| json!({"index": idx, "embedding": emb}))
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({
        "object": "list",
        "data": data,
        "model": "test-model",
        "usage": {"prompt_tokens": 10, "total_tokens": 10}
    }))
}

#[tokio::test]
async fn test_single_text_embedding() {
    let (server, embedder) = mock_embedder(3).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![(0, vec![0.1, 0.2, 0.3])]))
        .mount(&server)
        .await;

    let result = embedder.embed_texts(&["hello".into()]).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], vec![0.1, 0.2, 0.3]);
}

#[tokio::test]
async fn test_batch_embedding() {
    let (server, embedder) = mock_embedder(2).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![
            (0, vec![1.0, 2.0]),
            (1, vec![3.0, 4.0]),
            (2, vec![5.0, 6.0]),
        ]))
        .mount(&server)
        .await;

    let texts: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    let result = embedder.embed_texts(&texts).await.unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], vec![1.0, 2.0]);
    assert_eq!(result[2], vec![5.0, 6.0]);
}

#[tokio::test]
async fn test_out_of_order_indexes_sorted() {
    let (server, embedder) = mock_embedder(2).await;

    // Server returns results out of order (index 1 before 0)
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![
            (1, vec![3.0, 4.0]),
            (0, vec![1.0, 2.0]),
        ]))
        .mount(&server)
        .await;

    let result = embedder
        .embed_texts(&["first".into(), "second".into()])
        .await
        .unwrap();
    // Should be sorted by index
    assert_eq!(result[0], vec![1.0, 2.0]);
    assert_eq!(result[1], vec![3.0, 4.0]);
}

#[tokio::test]
async fn test_empty_input_returns_empty() {
    let (_server, embedder) = mock_embedder(3).await;
    // No mock needed — empty input short-circuits before HTTP call
    let result = embedder.embed_texts(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_embed_query_delegates_to_embed_texts() {
    let (server, embedder) = mock_embedder(2).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![(0, vec![9.0, 8.0])]))
        .mount(&server)
        .await;

    let result = embedder.embed_query("question").await.unwrap();
    assert_eq!(result, vec![9.0, 8.0]);
}

#[tokio::test]
async fn test_dimension_mismatch_error() {
    let (server, embedder) = mock_embedder(3).await;

    // Return 2-dim when 3-dim expected
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![(0, vec![1.0, 2.0])]))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["test".into()])
        .await
        .expect_err("should reject wrong dimensions");
    let msg = format!("{err}");
    assert!(msg.contains("2 dimensions"), "error: {msg}");
    assert!(msg.contains("expected 3"), "error: {msg}");
}

#[tokio::test]
async fn test_count_mismatch_error() {
    let (server, embedder) = mock_embedder(2).await;

    // Return 1 vector for 2 inputs
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![(0, vec![1.0, 2.0])]))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["a".into(), "b".into()])
        .await
        .expect_err("should reject count mismatch");
    let msg = format!("{err}");
    assert!(msg.contains("1 vectors"), "error: {msg}");
    assert!(msg.contains("2 inputs"), "error: {msg}");
}

#[tokio::test]
async fn test_duplicate_index_error() {
    let (server, embedder) = mock_embedder(2).await;

    // Duplicate index 0
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(embedding_response(vec![
            (0, vec![1.0, 2.0]),
            (0, vec![3.0, 4.0]),
        ]))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["a".into(), "b".into()])
        .await
        .expect_err("should reject duplicate indexes");
    let msg = format!("{err}");
    assert!(msg.contains("invalid index"), "error: {msg}");
}

#[tokio::test]
async fn test_server_error_500() {
    let (server, embedder) = mock_embedder(3).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["test".into()])
        .await
        .expect_err("should propagate 500");
    let msg = format!("{err}");
    assert!(msg.contains("500"), "error: {msg}");
}

#[tokio::test]
async fn test_client_error_400() {
    let (server, embedder) = mock_embedder(3).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["test".into()])
        .await
        .expect_err("should propagate 400");
    let msg = format!("{err}");
    assert!(msg.contains("rejected request"), "error: {msg}");
}

#[tokio::test]
async fn test_rate_limit_429() {
    let (server, embedder) = mock_embedder(3).await;

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&server)
        .await;

    let err = embedder
        .embed_texts(&["test".into()])
        .await
        .expect_err("should propagate 429");
    let msg = format!("{err}");
    assert!(msg.contains("rate limited"), "error: {msg}");
}

#[tokio::test]
async fn test_api_key_sent_as_bearer() {
    let server = MockServer::start().await;
    let embedder = OpenAiCompatibleEmbedder::new(
        server.uri(),
        "test-model".into(),
        Some("sk-test-key".into()),
        2,
    )
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .and(wiremock::matchers::header("authorization", "Bearer sk-test-key"))
        .respond_with(embedding_response(vec![(0, vec![1.0, 2.0])]))
        .mount(&server)
        .await;

    let result = embedder.embed_texts(&["hello".into()]).await.unwrap();
    assert_eq!(result.len(), 1);
}

#[tokio::test]
async fn test_model_id_and_dimensions() {
    let embedder =
        OpenAiCompatibleEmbedder::new("http://localhost:1".into(), "my-model".into(), None, 768)
            .unwrap();
    assert_eq!(embedder.model_id(), "my-model");
    assert_eq!(embedder.dimensions(), 768);
}
