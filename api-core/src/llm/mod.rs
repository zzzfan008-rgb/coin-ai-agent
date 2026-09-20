//! LLM client module — OpenAI-compatible interface for MiniMax / DeepSeek.

pub mod client;
pub mod messages;
pub mod tools;

#[allow(unused_imports)]
pub use client::LlmClient;
#[allow(unused_imports)]
pub use messages::ChatMessage;
#[allow(unused_imports)]
pub use tools::ToolDefinition;
