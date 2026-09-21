//! Agent Loop — orchestrates message → LLM → tool_calls → execute → result → reply.

mod engine;

pub use engine::{AgentConfig, AgentEngine};
