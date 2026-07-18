use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct LlmCompletion {
    pub content: String,
    pub model: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub latency_ms: i64,
    pub used_fallback: bool,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<LlmCompletion>;
    fn model_name(&self) -> &str;
    fn kind(&self) -> &'static str;
}

/// Deterministic fallback used when no LLM API key is configured.
pub struct FallbackLlmClient {
    pub model: String,
}

#[async_trait]
impl LlmClient for FallbackLlmClient {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<LlmCompletion> {
        let started = Instant::now();
        let content = fallback_reply(messages);
        let prompt_tokens = estimate_tokens(
            &messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let completion_tokens = estimate_tokens(&content);
        Ok(LlmCompletion {
            content,
            model: self.model.clone(),
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            latency_ms: started.elapsed().as_millis() as i64,
            used_fallback: true,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn kind(&self) -> &'static str {
        "fallback"
    }
}

/// Pure function so unit tests can assert the shipped fallback path without network.
pub fn fallback_reply(messages: &[ChatMessage]) -> String {
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("");

    let action = last_user.trim();
    if action == "__skip_dont_know__" {
        return "没关系。那换个容易一点的：那时候家里谁经常陪着你？".into();
    }
    if action == "__skip_change_question__" {
        return "好，我们换一个。你小时候最喜欢吃的一样东西是什么？".into();
    }
    if action == "__skip_prefer_not__" {
        return "理解，我们不谈这个。你还记得当时住的房子是什么样的吗？".into();
    }

    let snippet: String = last_user.chars().take(24).collect();
    if snippet.is_empty() {
        "先从一件小事开始：你小时候住的地方，门口是什么样的？".into()
    } else {
        format!("谢谢你分享。关于「{snippet}」——当时具体是在哪里发生的？")
    }
}

pub fn estimate_tokens(text: &str) -> i32 {
    let chars = text.chars().count().max(1) as f32;
    (chars / 1.5).ceil() as i32
}

/// Extract assistant text from OpenAI-compatible JSON, including Qwen reasoning models.
pub fn extract_message_content(resp: &serde_json::Value) -> anyhow::Result<String> {
    let message = resp
        .pointer("/choices/0/message")
        .ok_or_else(|| anyhow::anyhow!("LLM response missing choices[0].message"))?;

    // 1) content as string
    if let Some(s) = message.get("content").and_then(|v| v.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }

    // 2) content as array of parts: [{"type":"text","text":"..."}]
    if let Some(arr) = message.get("content").and_then(|v| v.as_array()) {
        let mut parts = Vec::new();
        for p in arr {
            if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                if !t.trim().is_empty() {
                    parts.push(t.trim());
                }
            } else if let Some(t) = p.as_str() {
                if !t.trim().is_empty() {
                    parts.push(t.trim());
                }
            }
        }
        if !parts.is_empty() {
            return Ok(parts.join("\n"));
        }
    }

    // 3) Qwen / reasoning models sometimes put final answer in other fields
    for key in ["reasoning_content", "refusal", "output_text"] {
        if let Some(s) = message.get(key).and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                // Prefer a short question-like trailing line if present
                return Ok(t.to_string());
            }
        }
    }

    // 4) some gateways put text at choices[0].text
    if let Some(s) = resp.pointer("/choices/0/text").and_then(|v| v.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }

    let snippet = resp.to_string();
    let snippet: String = snippet.chars().take(400).collect();
    Err(anyhow::anyhow!(
        "LLM response missing usable content; body snippet: {snippet}"
    ))
}

/// Normalize OpenAI-compatible base URL (strip trailing slash / accidental /chat/completions).
pub fn normalize_api_base(base: &str) -> String {
    let mut b = base.trim().trim_end_matches('/').to_string();
    if b.ends_with("/chat/completions") {
        b = b.trim_end_matches("/chat/completions").trim_end_matches('/').to_string();
    }
    b
}

pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
    model: String,
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<LlmCompletion> {
        let started = Instant::now();
        let url = format!(
            "{}/chat/completions",
            normalize_api_base(&self.base)
        );
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.4,
        });

        // One retry on transient network / 429 / 5xx
        let mut last_err = None;
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(400 * attempt as u64)).await;
            }
            match self.do_request(&url, &body, started).await {
                Ok(c) => return Ok(c),
                Err(e) => {
                    let msg = e.to_string();
                    let retryable = msg.contains("429")
                        || msg.contains("500")
                        || msg.contains("502")
                        || msg.contains("503")
                        || msg.contains("timeout")
                        || msg.contains("timed out")
                        || msg.contains("connection")
                        || msg.contains("error sending request");
                    last_err = Some(e);
                    if !retryable {
                        break;
                    }
                    tracing::warn!(attempt, error = %msg, "LLM request failed; retrying");
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("LLM request failed")))
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn kind(&self) -> &'static str {
        "openai_compatible"
    }
}

impl OpenAiCompatibleClient {
    async fn do_request(
        &self,
        url: &str,
        body: &serde_json::Value,
        started: Instant,
    ) -> anyhow::Result<LlmCompletion> {
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("LLM HTTP transport error calling {url}: {e}"))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("LLM read body failed: {e}"))?;

        if !status.is_success() {
            let snippet: String = text.chars().take(500).collect();
            return Err(anyhow::anyhow!(
                "LLM HTTP {status} from {url}: {snippet}"
            ));
        }

        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            let snippet: String = text.chars().take(300).collect();
            anyhow::anyhow!("LLM JSON parse failed: {e}; body: {snippet}")
        })?;

        let content = extract_message_content(&value)?;

        let prompt_tokens = value
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(|| {
                estimate_tokens(
                    &body["messages"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                )
            });
        let completion_tokens = value
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(|| estimate_tokens(&content));
        let total_tokens = value
            .pointer("/usage/total_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(prompt_tokens + completion_tokens);

        let model = value
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.model)
            .to_string();

        Ok(LlmCompletion {
            content,
            model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            latency_ms: started.elapsed().as_millis() as i64,
            used_fallback: false,
        })
    }
}

pub fn build_llm_client(
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: &str,
) -> Arc<dyn LlmClient> {
    match (api_base, api_key) {
        (Some(base), Some(key)) if !base.trim().is_empty() && !key.trim().is_empty() => {
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(90))
                .connect_timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            Arc::new(OpenAiCompatibleClient {
                http,
                base: normalize_api_base(base),
                api_key: key.trim().to_string(),
                model: model.trim().to_string(),
            })
        }
        _ => Arc::new(FallbackLlmClient {
            model: format!("{}-fallback", model.trim()),
        }),
    }
}

pub fn build_llm_client_from_config(config: &Config) -> Arc<dyn LlmClient> {
    build_llm_client(
        config.llm_api_base.as_deref(),
        config.llm_api_key.as_deref(),
        &config.llm_model,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fallback_asks_concrete_follow_up() {
        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: "我小时候走路上学".into(),
        }];
        let reply = fallback_reply(&msgs);
        assert!(reply.contains("走路上学") || reply.contains("哪里"));
        assert!(!reply.is_empty());
    }

    #[test]
    fn fallback_handles_skip_actions() {
        let msgs = vec![ChatMessage {
            role: "user".into(),
            content: "__skip_dont_know__".into(),
        }];
        assert!(fallback_reply(&msgs).contains("换个"));
    }

    #[test]
    fn extract_string_content() {
        let v = json!({"choices":[{"message":{"content":"下一问？"}}]});
        assert_eq!(extract_message_content(&v).unwrap(), "下一问？");
    }

    #[test]
    fn extract_array_content() {
        let v = json!({"choices":[{"message":{"content":[{"type":"text","text":"你好"}]}}]});
        assert_eq!(extract_message_content(&v).unwrap(), "你好");
    }

    #[test]
    fn extract_reasoning_content_fallback() {
        let v = json!({"choices":[{"message":{"content":null,"reasoning_content":"思考后：问细节"}}]});
        assert!(extract_message_content(&v).unwrap().contains("细节"));
    }

    #[test]
    fn normalize_base_strips_chat_path() {
        assert_eq!(
            normalize_api_base("https://x.com/v1/chat/completions/"),
            "https://x.com/v1"
        );
    }
}
