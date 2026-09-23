//! Agent Loop engine — drives a conversation turn through the LLM.
//!
//! Loop:
//!   1. Build messages (system prompt + RAG context injection)
//!   2. Call LLM
//!   3. If LLM returns tool_calls → PreToolCall Hook → execute → append results → 2
//!   4. Return final assistant message
//!
//! `max_turns` caps tool-call depth to prevent infinite loops.

use std::sync::Arc;

use crate::api::handlers::UserContext;
use crate::error::{AppError, Result};
use crate::llm::client::ToolCall;
use crate::llm::messages::ChatMessage;
use crate::llm::tools::ToolDefinition;
use crate::llm::LlmClient;
use crate::middleware;
use crate::session::SessionStore;

/// Token usage returned from an agent turn.
#[derive(Debug, Clone, Default)]
pub struct TurnUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

use crate::rag;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub max_turns: usize,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 30,
            temperature: Some(0.7),
            max_tokens: Some(32768),
        }
    }
}

pub struct AgentEngine {
    config: AgentConfig,
    llm: Arc<LlmClient>,
    #[allow(dead_code)]
    session_store: Arc<SessionStore>,
    rag: Option<Arc<rag::RagRetriever>>,
}

impl AgentEngine {
    pub fn new(
        config: AgentConfig,
        llm: Arc<LlmClient>,
        session_store: Arc<SessionStore>,
        rag: Option<Arc<rag::RagRetriever>>,
    ) -> Self {
        Self { config, llm, session_store, rag }
    }

    /// Run one full turn until the LLM produces a message with no tool calls.
    /// Returns the assistant content and aggregated token usage.
    pub async fn run_turn(
        &self,
        messages: &[ChatMessage],
        model: Option<&str>,
        tools: Option<&[ToolDefinition]>,
        user_ctx: &UserContext,
        knowledge_collections: Option<&[String]>,
    ) -> Result<(String, TurnUsage)> {
        let mut conversation: Vec<ChatMessage> = messages.to_vec();
        let mut total_usage = TurnUsage::default();

        for turn in 1..=self.config.max_turns {
            self.maybe_inject_rag_context(
                &mut conversation,
                knowledge_collections,
                user_ctx,
            )
                .await;

            let response = self
                .llm
                .chat(
                    &conversation,
                    model,
                    self.config.temperature,
                    self.config.max_tokens,
                    tools,
                )
                .await?;

            if let Some(usage) = response.usage {
                total_usage.prompt_tokens += usage.prompt_tokens;
                total_usage.completion_tokens += usage.completion_tokens;
                total_usage.total_tokens += usage.total_tokens;
            }

            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| AppError::LlmError("No choices in LLM response".to_string()))?;

            let assistant = choice.message;
            let Some(tool_calls) = assistant.tool_calls else {
                return Ok((assistant.content.unwrap_or_default(), total_usage));
            };

            tracing::info!(turn, n = tool_calls.len(), "LLM requested tool calls");

            // Run PreToolCall Hook + execution for each call in parallel.
            let tool_results = futures::future::join_all(
                tool_calls
                    .iter()
                    .map(|tc| execute_single_tool(tc, user_ctx)),
            )
            .await;

            // Persist the assistant turn (with tool_calls) in the conversation.
            conversation.push(ChatMessage {
                role: "assistant".to_string(),
                content: assistant.content,
                name: None,
                tool_call_id: None,
                tool_calls: Some(tool_calls),
            });

            for tr in tool_results {
                let tr = tr?;
                conversation.push(ChatMessage::tool(&tr.content, &tr.tool_call_id));
            }
        }

        Err(AppError::Internal(format!(
            "Max tool-call depth ({}) exceeded",
            self.config.max_turns
        )))
    }

    /// Prepend retrieved knowledge to the last user message.
    async fn maybe_inject_rag_context(
        &self,
        conversation: &mut Vec<ChatMessage>,
        collections: Option<&[String]>,
        user_ctx: &UserContext,
    ) {
        let (Some(rag), Some(colls)) = (&self.rag, collections) else {
            return;
        };
        if colls.is_empty() {
            return;
        }
        let Some(idx) = conversation.iter().rposition(|m| m.role == "user") else {
            return;
        };
        let original = conversation[idx].content.clone().unwrap_or_default();
        let context = rag
            .retrieve(&original, &user_ctx.org_id, &user_ctx.dept_id, 5)
            .await
            .unwrap_or_default();
        if context.is_empty() {
            return;
        }
        conversation[idx].content = Some(format!(
            "[Relevant knowledge]:\n{context}\n[/Relevant knowledge]\n\n{original}"
        ));
    }
}

#[derive(Debug)]
struct ToolResult {
    tool_call_id: String,
    content: String,
}

/// Authorization hook + tool dispatch for a single LLM tool call.
async fn execute_single_tool(tool_call: &ToolCall, user_ctx: &UserContext) -> Result<ToolResult> {
    let tool_name = tool_call.function.name.clone();
    let arguments: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    // ── PreToolCall Hook (Casbin boundary) ───────────────────────────────────
    middleware::pre_tool_call_check(user_ctx, &tool_name, &arguments).await?;

    tracing::info!(tool = %tool_name, "Executing tool");
    let store = crate::skill_engine::FASHION_STORE
        .get()
        .ok_or_else(|| AppError::Internal("FashionStore not initialised".into()))?;
    let content = crate::tool::execute_tool(&tool_name, &arguments, user_ctx, store).await?;

    Ok(ToolResult {
        tool_call_id: tool_call.id.clone(),
        content,
    })
}
