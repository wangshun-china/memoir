use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

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
    // Rough CJK-aware estimate: ~1.5 chars per token for mixed Chinese.
    let chars = text.chars().count().max(1) as f32;
    (chars / 1.5).ceil() as i32
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
        let url = format!("{}/chat/completions", self.base.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.4,
        });
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let content = resp
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("LLM response missing content"))?
            .to_string();

        let prompt_tokens = resp
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(|| {
                estimate_tokens(
                    &messages
                        .iter()
                        .map(|m| m.content.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            });
        let completion_tokens = resp
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(|| estimate_tokens(&content));
        let total_tokens = resp
            .pointer("/usage/total_tokens")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(prompt_tokens + completion_tokens);

        Ok(LlmCompletion {
            content,
            model: self.model.clone(),
            prompt_tokens,
            completion_tokens,
            total_tokens,
            latency_ms: started.elapsed().as_millis() as i64,
            used_fallback: false,
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn kind(&self) -> &'static str {
        "openai_compatible"
    }
}

pub fn build_llm_client(
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: &str,
) -> Arc<dyn LlmClient> {
    match (api_base, api_key) {
        (Some(base), Some(key)) if !base.is_empty() && !key.is_empty() => {
            Arc::new(OpenAiCompatibleClient {
                http: reqwest::Client::new(),
                base: base.to_string(),
                api_key: key.to_string(),
                model: model.to_string(),
            })
        }
        _ => Arc::new(FallbackLlmClient {
            model: format!("{model}-fallback"),
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
}
