//! Agent Loop — orchestrates message → LLM → tool_calls → execute → result → reply.

mod engine;
mod rag;

pub use engine::{AgentConfig, AgentEngine};
