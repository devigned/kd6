pub mod embedding;
pub mod error;
pub mod models;
pub mod provider;

pub use embedding::{
    auto_embed_content, auto_embed_query, auto_embed_update, content_to_text, EmbeddingProvider,
    NoopEmbedder,
};
pub use error::OmsError;
pub use provider::OmsProvider;
