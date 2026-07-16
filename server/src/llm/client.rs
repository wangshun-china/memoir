use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<String>;
}

/// Deterministic fallback used when no LLM API key is configured.
pub struct FallbackLlmClient;

#[async_trait]
impl LlmClient for FallbackLlmClient {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<String> {
        Ok(fallback_reply(messages))
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

    // Default: one short concrete follow-up, referencing a snippet of the answer.
    let snippet: String = last_user.chars().take(24).collect();
    if snippet.is_empty() {
        "先从一件小事开始：你小时候住的地方，门口是什么样的？".into()
    } else {
        format!("谢谢你分享。关于「{}」——当时具体是在哪里发生的？", snippet)
    }
}

pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
    base: String,
    api_key: String,
    model: String,
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn complete(&self, messages: &[ChatMessage]) -> anyhow::Result<String> {
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
        Ok(content)
    }
}

pub fn build_llm_client(config: &Config) -> Arc<dyn LlmClient> {
    if let (Some(base), Some(key)) = (&config.llm_api_base, &config.llm_api_key) {
        Arc::new(OpenAiCompatibleClient {
            http: reqwest::Client::new(),
            base: base.clone(),
            api_key: key.clone(),
            model: config.llm_model.clone(),
        })
    } else {
        Arc::new(FallbackLlmClient)
    }
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
