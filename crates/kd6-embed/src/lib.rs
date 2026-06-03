#[cfg(feature = "local")]
mod local;
mod openai_compatible;

#[cfg(feature = "local")]
pub use local::LocalEmbedder;
pub use openai_compatible::OpenAiCompatibleEmbedder;
