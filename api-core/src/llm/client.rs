//! LLM client — OpenAI-compatible REST client for MiniMax and DeepSeek.
//!
//! Both providers expose an OpenAI-compatible `/chat/completions` endpoint.
//! This client abstracts over provider selection via the `llm_provider` config
//! and supports per-request model overrides (e.g. `extra_body.model`).
//!
//! Supported features:
//!   - Function calling / tool use (OpenAI `tools` format)
//!   - Streaming SSE with proper frame buffering
//!   - 429 retry with exponential backoff (3 attempts)
//!   - Per-provider health check

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
#[derive(Clone)]
pub struct LlmClient {
    http: Client,
    default_provider: String,
    minimax: LlmProvider,
    deepseek: LlmProvider,
    /// Timeout in seconds (from config).
    timeout_secs: u64,
}

#[derive(Clone)]
struct LlmProvider {
    api_key: String,
    model: String,
    base_url: String,
}

impl LlmClient {
    /// Build a new LLM client. Returns an error if no API key is configured
    /// for the default provider (fail-fast — do not start with a missing key).
    pub fn new(config: &AppConfig) -> anyhow::Result<Self> {
        let timeout_secs = config.llm_timeout_secs;

        let http = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;

        let minimax = LlmProvider {
            api_key: config.minimax_api_key.clone(),
            model: config.minimax_model.clone(),
            base_url: config.minimax_base_url.clone(),
        };
        let deepseek = LlmProvider {
            api_key: config.deepseek_api_key.clone(),
            model: config.deepseek_model.clone(),
            base_url: config.deepseek_base_url.clone(),
        };

        let default_provider = config.llm_provider.clone();

        // Fail-fast: the default provider MUST have an API key.
        let active = match default_provider.as_str() {
            "deepseek" => &deepseek,
            _ => &minimax,
        };
        if active.api_key.is_empty() {
            anyhow::bail!(
                "LLM_PROVIDER={} but {} API key is not set. \
                 Set {} in your .env file.",
                default_provider,
                default_provider,
                match default_provider.as_str() {
                    "deepseek" => "DEEPSEEK_API_KEY",
                    _ => "MINIMAX_API_KEY",
                }
            );
        }

        Ok(Self { http, default_provider, minimax, deepseek, timeout_secs })
    }

    /// Return the provider and model to use for a given model name.
    /// If the model name matches a known provider's default model, route to that provider.
    /// Otherwise, use the default provider.
    fn resolve_provider(&self, model: Option<&str>) -> (&LlmProvider, String) {
        let model = model.unwrap_or_default();
        // Route by known model prefixes/names.
        if model.starts_with("deepseek") {
            (&self.deepseek, model.to_string())
        } else if model.starts_with("minimax") || model.starts_with("MiniMax") {
            (&self.minimax, model.to_string())
        } else {
            // Use default provider.
            match self.default_provider.as_str() {
                "deepseek" => (&self.deepseek, model.to_string()),
                _ => (&self.minimax, model.to_string()),
            }
        }
    }

    fn build_body(
        &self,
        provider: &LlmProvider,
        messages: &[ChatMessage],
        stream: bool,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> serde_json::Value {
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

    /// Non-streaming chat completion with 429 retry + exponential backoff.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        model: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<ChatCompletionResponse> {
        let (provider, resolved_model) = self.resolve_provider(model);
        let body = self.build_body(provider, messages, false, temperature, max_tokens, tools);
        // Override model in body with resolved model name.
        let body = {
            let mut b = body;
            b["model"] = serde_json::json!(resolved_model);
            b
        };

        self.do_request_with_retry(provider, &body).await
    }

    async fn do_request_with_retry(
        &self,
        provider: &LlmProvider,
        body: &serde_json::Value,
    ) -> Result<ChatCompletionResponse> {
        let url = format!("{}/chat/completions", provider.base_url);
        let mut attempt = 0;

        loop {
            attempt += 1;
            let resp = self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", provider.api_key))
                .json(body)
                .send()
                .await
                .map_err(AppError::HttpError)?;

            let status = resp.status();

            if status.is_success() {
                let text = resp.text().await.map_err(AppError::HttpError)?;
                return serde_json::from_str(&text)
                    .map_err(|e| AppError::LlmError(format!("Parse failure: {e}; raw: {text}")));
            }

            if status.as_u16() == 429 && attempt < 3 {
                // Exponential backoff: 1s, 2s.
                let delay_ms = 1000 * 2u64.pow(attempt - 1);
                tracing::warn!(
                    "LLM 429 (attempt {}/3), backing off {}ms before retry",
                    attempt, delay_ms
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                continue;
            }

            // Non-retryable failure (or exhausted retries).
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::LlmError(format!("LLM API {status}: {text}")));
        }
    }

    /// Streaming chat completion. Yields parsed SSE chunks; stream ends on [DONE].
    ///
    /// Network frames are buffered and split on SSE event boundaries
    /// (`\n\n` / `\r\n\r\n`) so partial frames are handled correctly.
    ///
    /// **Note:** SSE chunks from providers do not include usage. Call the
    /// non-streaming `chat()` separately if you need accurate token counts.
    pub async fn chat_streaming(
        &self,
        messages: &[ChatMessage],
        model: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<impl Stream<Item = Result<SseChunk>>> {
        let (provider, resolved_model) = self.resolve_provider(model);
        let body = self.build_body(provider, messages, true, temperature, max_tokens, tools);
        let body = {
            let mut b = body;
            b["model"] = serde_json::json!(resolved_model);
            b
        };

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

    /// Health check for the default provider.
    pub async fn health_check(&self) -> bool {
        self.health_check_provider(&self.default_provider).await
    }

    /// Health check for a specific provider by name ("minimax" | "deepseek").
    pub async fn health_check_provider(&self, provider_name: &str) -> bool {
        let provider = match provider_name {
            "deepseek" => &self.deepseek,
            _ => &self.minimax,
        };

        if provider.api_key.is_empty() {
            return false;
        }

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

    /// Check health of all configured providers. Returns a map of provider → healthy.
    pub async fn health_check_all(&self) -> std::collections::HashMap<String, bool> {
        let mut result = std::collections::HashMap::new();
        result.insert(
            "minimax".to_string(),
            self.health_check_provider("minimax").await,
        );
        result.insert(
            "deepseek".to_string(),
            self.health_check_provider("deepseek").await,
        );
        result
    }
}

/// Pick whichever separator appears first in the buffer.
fn earliest(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> Option<(usize, usize)> {
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

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: usize,
    #[serde(default)]
    pub completion_tokens: usize,
    #[serde(default)]
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
    /// Some providers (e.g. DeepSeek) include usage in the final chunk.
    #[serde(default)]
    pub usage: Option<Usage>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(provider: &str, minimax_key: &str, deepseek_key: &str) -> AppConfig {
        AppConfig {
            app_env: "test".into(),
            log_level: "info".into(),
            database_url: String::new(),
            db_max_connections: 1,
            redis_url: String::new(),
            qdrant_url: String::new(),
            core_port: 0,
            llm_provider: provider.into(),
            llm_timeout_secs: 60,
            minimax_api_key: minimax_key.into(),
            minimax_model: "MiniMax-Text-01".into(),
            minimax_base_url: "https://api.minimaxi.chat/v1".into(),
            deepseek_api_key: deepseek_key.into(),
            deepseek_model: "deepseek-chat".into(),
            deepseek_base_url: "https://api.deepseek.com/v1".into(),
            minio_endpoint: String::new(),
            minio_bucket: String::new(),
            minio_access_key: String::new(),
            minio_secret_key: String::new(),
        }
    }

    #[test]
    fn fails_fast_when_default_provider_key_missing() {
        // minimax default, no minimax key → error
        let cfg = make_config("minimax", "", "");
        assert!(LlmClient::new(&cfg).is_err());

        // deepseek default, no deepseek key → error
        let cfg = make_config("deepseek", "some-minimax", "");
        assert!(LlmClient::new(&cfg).is_err());
    }

    #[test]
    fn builds_when_default_provider_key_present() {
        let cfg = make_config("minimax", "mm-key", "");
        assert!(LlmClient::new(&cfg).is_ok());

        let cfg = make_config("deepseek", "", "ds-key");
        assert!(LlmClient::new(&cfg).is_ok());
    }

    #[test]
    fn resolves_provider_by_model_prefix() {
        let cfg = make_config("minimax", "mm-key", "ds-key");
        let client = LlmClient::new(&cfg).unwrap();

        // deepseek model routes to deepseek even though default is minimax
        let (p, model) = client.resolve_provider(Some("deepseek-chat"));
        assert_eq!(p.api_key, "ds-key");
        assert_eq!(model, "deepseek-chat");

        // unknown model falls back to default provider (minimax)
        let (p, _) = client.resolve_provider(Some("some-random-model"));
        assert_eq!(p.api_key, "mm-key");
    }

    fn config_pointing_at(provider: &str, base_url: &str) -> AppConfig {
        let mut cfg = make_config(provider, "test-key", "test-key");
        match provider {
            "deepseek" => cfg.deepseek_base_url = base_url.into(),
            _ => cfg.minimax_base_url = base_url.into(),
        }
        cfg
    }

    #[tokio::test]
    async fn parses_content_and_usage_from_real_http() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let server = wiremock::MockServer::start().await;

        let body = serde_json::json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "created": 1700000000u64,
            "model": "MiniMax-Text-01",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "真实LLM回复内容"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 12, "completion_tokens": 7, "total_tokens": 19}
        });

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let cfg = config_pointing_at("minimax", &server.uri());
        let client = LlmClient::new(&cfg).unwrap();

        let resp = client
            .chat(&[ChatMessage::user("你好")], None, None, None, None)
            .await
            .expect("chat should succeed");

        let content = resp.choices[0].message.content.clone().unwrap();
        assert_eq!(content, "真实LLM回复内容");
        let usage = resp.usage.expect("usage must be parsed");
        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.completion_tokens, 7);
        assert_eq!(usage.total_tokens, 19);
    }

    #[tokio::test]
    async fn retries_on_429_then_succeeds() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        let ok_body = serde_json::json!({
            "id": "x", "object": "chat.completion", "created": 1u64, "model": "m",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        });

        // First call 429, subsequent calls 200.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_body))
            .mount(&server)
            .await;

        let cfg = config_pointing_at("minimax", &server.uri());
        let client = LlmClient::new(&cfg).unwrap();

        let resp = client
            .chat(&[ChatMessage::user("hi")], None, None, None, None)
            .await
            .expect("should succeed after retry");
        assert_eq!(resp.choices[0].message.content.clone().unwrap(), "ok");

        // Exactly 2 requests hit the server (1 failed + 1 retry).
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn maps_upstream_failure_to_llm_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal boom"))
            .mount(&server)
            .await;

        let cfg = config_pointing_at("minimax", &server.uri());
        let client = LlmClient::new(&cfg).unwrap();

        let err = client
            .chat(&[ChatMessage::user("hi")], None, None, None, None)
            .await
            .expect_err("should fail");
        // LlmError maps to HTTP 502 at the handler boundary.
        assert!(matches!(err, AppError::LlmError(_)));
    }
}
