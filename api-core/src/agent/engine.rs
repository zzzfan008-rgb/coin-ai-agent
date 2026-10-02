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
use crate::llm::client::{FunctionCall, ToolCall};
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
        Self {
            config,
            llm,
            session_store,
            rag,
        }
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
            self.maybe_inject_rag_context(&mut conversation, knowledge_collections, user_ctx)
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
            let (tool_calls, assistant_content) = match assistant.tool_calls {
                Some(tc) => (tc, assistant.content),
                None => {
                    // Fallback: some models (e.g. deepseek-v4-flash) emit tool
                    // calls as `<function_calls><invoke name="...">` text
                    // instead of the native tool_calls field. Parse them so the
                    // skills actually execute instead of leaking raw XML to the
                    // user.
                    let content = assistant.content.unwrap_or_default();
                    match parse_xml_tool_calls(&content) {
                        Some((calls, stripped)) => {
                            tracing::info!(n = calls.len(), "Parsed XML function_calls from content");
                            (calls, Some(stripped))
                        }
                        None => return Ok((content, total_usage)),
                    }
                }
            };

            tracing::info!(turn, n = tool_calls.len(), "LLM requested tool calls");

            // Run PreToolCall Hook + execution for each call in parallel.
            let tool_results = futures::future::join_all(
                tool_calls
                    .iter()
                    .map(|tc| execute_single_tool(tc, user_ctx, self.rag.as_deref())),
            )
            .await;

            // Persist the assistant turn (with tool_calls) in the conversation.
            conversation.push(ChatMessage {
                role: "assistant".to_string(),
                content: assistant_content,
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
        let Some(rag) = &self.rag else {
            return;
        };
        // RAG retrieval is gated only on the retriever being configured.
        // Qdrant search already filters by org/dept (tenant isolation), so
        // an empty/absent knowledge_collections list no longer disables
        // retrieval — it previously made RAG silently never trigger.
        let _ = collections;
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
async fn execute_single_tool(
    tool_call: &ToolCall,
    user_ctx: &UserContext,
    rag: Option<&rag::RagRetriever>,
) -> Result<ToolResult> {
    let tool_name = tool_call.function.name.clone();
    let arguments: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    // ── PreToolCall Hook (Casbin boundary) ───────────────────────────────────
    middleware::pre_tool_call_check(user_ctx, &tool_name, &arguments).await?;

    tracing::info!(tool = %tool_name, "Executing tool");
    let store = crate::skill_engine::FASHION_STORE
        .get()
        .ok_or_else(|| AppError::Internal("FashionStore not initialised".into()))?;
    let content = crate::tool::execute_tool(&tool_name, &arguments, user_ctx, store, rag).await?;

    Ok(ToolResult {
        tool_call_id: tool_call.id.clone(),
        content,
    })
}

/// Parse tool calls emitted as XML text instead of the native tool_calls
/// field. Some models (e.g. deepseek-v4-flash) emit function-call intent as
/// a function_calls block: <function_calls> containing
/// <invoke name="..."> rows whose <parameter name="..."> children
/// carry the arguments. Returns the parsed calls plus the content with the
/// block stripped, so the raw XML never reaches the user.
fn parse_xml_tool_calls(content: &str) -> Option<(Vec<ToolCall>, String)> {
    let open = "<function_calls>";
    let close = "</function_calls>";
    let start = content.find(open)?;
    let end = content[start..].find(close)? + start + close.len();
    let block = &content[start..end];

    let mut calls = Vec::new();
    let mut rest = block;
    let iopen = "<invoke name=\"";
    let iclose = "</invoke>";
    while let Some(ipos) = rest.find(iopen) {
        let after_name = &rest[ipos + iopen.len()..];
        let name_end = after_name.find('"')?;
        let name = after_name[..name_end].to_string();
        // skip from the name's closing quote to the tag's '>'
        let after_quote = &rest[ipos + iopen.len() + name_end + 1..];
        let gt = after_quote.find('>')?;
        let body_start = ipos + iopen.len() + name_end + 1 + gt + 1;
        let body_end_rel = rest[body_start..].find(iclose)?;
        let body = &rest[body_start..body_start + body_end_rel];

        let mut params = serde_json::Map::new();
        let mut prest = body;
        let popen = "<parameter name=\"";
        let pclose = "</parameter>";
        while let Some(ppos) = prest.find(popen) {
            let pafter = &prest[ppos + popen.len()..];
            let pn_end = pafter.find('"')?;
            let pname = pafter[..pn_end].to_string();
            let after_pq = &prest[ppos + popen.len() + pn_end + 1..];
            let pgt = after_pq.find('>')?;
            let vstart = ppos + popen.len() + pn_end + 1 + pgt + 1;
            let vend_rel = prest[vstart..].find(pclose).unwrap_or(prest.len() - vstart);
            let raw = prest[vstart..vstart + vend_rel].trim().to_string();
            let val = match raw.parse::<i64>() {
                Ok(n) => serde_json::Value::Number(n.into()),
                Err(_) => match raw.parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
                    Some(num) => serde_json::Value::Number(num),
                    None => serde_json::Value::String(raw),
                },
            };
            params.insert(pname, val);
            prest = &prest[vstart + vend_rel + pclose.len()..];
        }

        calls.push(ToolCall {
            id: format!("xmlcall_{}", calls.len()),
            call_type: "function".to_string(),
            function: FunctionCall {
                name,
                arguments: serde_json::Value::Object(params).to_string(),
            },
        });
        rest = &rest[body_start + body_end_rel + iclose.len()..];
    }

    if calls.is_empty() {
        return None;
    }
    let stripped = (content[..start].to_string() + &content[end..]).trim().to_string();
    Some((calls, stripped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_tool_calls() {
        let content = format!(
            "我来帮你查询{lt}面料的信息。{nl}{nl}{lt}function_calls{gt}{nl}{lt}invoke name=\"skill_fabric_query_search_fabric\"{gt}{nl}{lt}parameter name=\"query\"{gt}竹纤维{lt}/parameter{gt}{nl}{lt}parameter name=\"limit\"{gt}10{lt}/parameter{gt}{nl}{lt}/invoke{gt}{nl}{lt}/function_calls{gt}",
            lt = char::from(60u8), gt = char::from(62u8), nl = "\n"
        );
        let (calls, stripped) = parse_xml_tool_calls(&content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "skill_fabric_query_search_fabric");
        let args: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
        assert_eq!(args["query"], "竹纤维");
        assert_eq!(args["limit"], 10);
        assert!(!stripped.contains("function_calls"));
        assert!(stripped.contains("我来帮你查询"));
    }

    #[test]
    fn test_parse_xml_tool_calls_none() {
        assert!(parse_xml_tool_calls("普通文本回复，没有工具调用。").is_none());
    }
}
