//! LLM client — OpenAI-compatible REST client for MiniMax and DeepSeek.
//!
//! Both providers expose an OpenAI-compatible `/chat/completions` endpoint.
//! This client abstracts over provider selection via the `llm_provider` config.
//!
//! Supported features:
//!   - Function calling / tool use (OpenAI `tools` format)
//!   - Streaming SSE with proper frame buffering
//!   - Provider health check

use std::time::Duration;

use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;

use crate::config::AppConfig;
use crate::error::{AppError, Result};

use super::messages::ChatMessage;
use super::tools::ToolDefinition;

/// Unified LLM client wrapping MiniMax and DeepSeek.
pub struct LlmClient {
    http: Client,
    provider: String,
    minimax: LlmProvider,
    deepseek: LlmProvider,
}

#[derive(Clone)]
struct LlmProvider {
    api_key: String,
    model: String,
    base_url: String,
}

impl LlmClient {
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(config.llm_timeout_secs))
            .build()?;

        Ok(Self {
            http,
            provider: config.llm_provider.clone(),
            minimax: LlmProvider {
                api_key: config.minimax_api_key.clone(),
                model: config.minimax_model.clone(),
                base_url: config.minimax_base_url.clone(),
            },
            deepseek: LlmProvider {
                api_key: config.deepseek_api_key.clone(),
                model: config.deepseek_model.clone(),
                base_url: config.deepseek_base_url.clone(),
            },
        })
    }

    fn active_provider(&self) -> &LlmProvider {
        match self.provider.as_str() {
            "deepseek" => &self.deepseek,
            _ => &self.minimax,
        }
    }

    fn build_body(
        &self,
        messages: &[ChatMessage],
        stream: bool,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> serde_json::Value {
        let provider = self.active_provider();
        let mut body = serde_json::json!({
            "model": provider.model,
            "messages": messages.iter().map(ChatMessage::to_json).collect::<Vec<_>>(),
            "stream": stream,
        });
        if let Some(t) = temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(mt) = max_tokens {
            body["max_tokens"] = serde_json::json!(mt);
        }
        if let Some(tl) = tools {
            body["tools"] = serde_json::json!(tl);
        }
        body
    }

    /// Non-streaming chat completion.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<ChatCompletionResponse> {
        let provider = self.active_provider();
        let body = self.build_body(messages, false, temperature, max_tokens, tools);

        let resp = self
            .http
            .post(format!("{}/chat/completions", provider.base_url))
            .header("Authorization", format!("Bearer {}", provider.api_key))
            .json(&body)
            .send()
            .await
            .map_err(AppError::HttpError)?;

        let status = resp.status();
        let text = resp.text().await.map_err(AppError::HttpError)?;
        if !status.is_success() {
            return Err(AppError::LlmError(format!("LLM API {status}: {text}")));
        }

        serde_json::from_str(&text)
            .map_err(|e| AppError::LlmError(format!("Parse failure: {e}; raw: {text}")))
    }

    /// Streaming chat completion. Yields parsed SSE chunks; stream ends on [DONE].
    ///
    /// Network frames are buffered and split on SSE event boundaries
    /// (`\n\n` / `\r\n\r\n`) so partial frames are handled correctly.
    pub async fn chat_streaming(
        &self,
        messages: &[ChatMessage],
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<impl Stream<Item = Result<SseChunk>>> {
        let provider = self.active_provider();
        let body = self.build_body(messages, true, temperature, max_tokens, tools);

        let resp = self
            .http
            .post(format!("{}/chat/completions", provider.base_url))
            .header("Authorization", format!("Bearer {}", provider.api_key))
            .header("Accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(AppError::HttpError)?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::LlmError(format!("LLM stream {status}: {text}")));
        }

        let mut raw = resp.bytes_stream();

        let s = async_stream::stream! {
            let mut buf = String::new();

            while let Some(frame) = raw.next().await {
                let bytes = match frame {
                    Ok(b) => b,
                    Err(e) => {
                        yield Err(AppError::HttpError(e));
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&bytes));

                // Drain all complete SSE events currently in the buffer.
                loop {
                    let sep_lf = buf.find("\n\n").map(|p| (p, 2));
                    let sep_crlf = buf.find("\r\n\r\n").map(|p| (p, 4));
                    let Some((sep_pos, sep_len)) = earliest(sep_lf, sep_crlf) else {
                        break;
                    };

                    let event_text: String = buf.drain(..sep_pos + sep_len).collect();
                    // SSE spec: one or more `data:` lines, concatenated with "\n".
                    let data: String = event_text
                        .lines()
                        .filter_map(|l| {
                            let t = l.trim_start();
                            t.strip_prefix("data:").map(|rest| rest.trim_start())
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    if data == "[DONE]" {
                        return;
                    }
                    if data.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<SseChunk>(&data) {
                        Ok(chunk) => yield Ok(chunk),
                        Err(e) => {
                            yield Err(AppError::LlmError(format!("SSE parse: {e}")));
                            return;
                        }
                    }
                }
            }
        };

        Ok(s)
    }

    /// Minimal ping against the active provider.
    pub async fn health_check(&self) -> bool {
        let provider = self.active_provider();
        let body = serde_json::json!({
            "model": provider.model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
        });
        self.http
            .post(format!("{}/chat/completions", provider.base_url))
            .header("Authorization", format!("Bearer {}", provider.api_key))
            .json(&body)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

/// Pick whichever separator appears first in the buffer.
fn earliest(
    a: Option<(usize, usize)>,
    b: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x.0 <= y.0 { x } else { y }),
        (x, None) => x,
        (None, y) => y,
    }
}

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Choice {
    pub index: usize,
    pub message: AssistantMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssistantMessage {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Chunk from a streaming chat completion.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SseChunk {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<u64>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<SseChoice>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SseChoice {
    pub index: usize,
    pub delta: SseDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SseDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<SseToolCall>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SseToolCall {
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub call_type: Option<String>,
    #[serde(default)]
    pub function: Option<SseFunctionCall>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SseFunctionCall {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
