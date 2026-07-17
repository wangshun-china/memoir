use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::error::AppResult;
use crate::llm::client::{build_llm_client, LlmClient};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfigView {
    pub api_base: String,
    pub api_key_set: bool,
    /// Masked key for UI; never full secret.
    pub api_key_masked: String,
    pub model: String,
    pub mode: String,
    pub has_live_client: bool,
}

#[derive(Clone)]
pub struct LlmRuntime {
    pub client: Arc<dyn LlmClient>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub model: String,
}

impl LlmRuntime {
    pub fn from_parts(
        api_base: Option<String>,
        api_key: Option<String>,
        model: String,
    ) -> Self {
        let client = build_llm_client(api_base.as_deref(), api_key.as_deref(), &model);
        Self {
            client,
            api_base,
            api_key,
            model,
        }
    }

    pub fn view(&self) -> AiConfigView {
        let key = self.api_key.clone().unwrap_or_default();
        let masked = if key.is_empty() {
            String::new()
        } else if key.len() <= 8 {
            "********".into()
        } else {
            format!("{}…{}", &key[..4], &key[key.len() - 4..])
        };
        AiConfigView {
            api_base: self.api_base.clone().unwrap_or_default(),
            api_key_set: !key.is_empty(),
            api_key_masked: masked,
            model: self.model.clone(),
            mode: self.client.kind().to_string(),
            has_live_client: self.client.kind() != "fallback",
        }
    }
}

pub async fn seed_settings_from_env(pool: &PgPool, config: &Config) -> AppResult<()> {
    upsert_setting(
        pool,
        "llm_api_base",
        config.llm_api_base.as_deref().unwrap_or(""),
        false,
    )
    .await?;
    upsert_setting(
        pool,
        "llm_api_key",
        config.llm_api_key.as_deref().unwrap_or(""),
        false,
    )
    .await?;
    upsert_setting(pool, "llm_model", &config.llm_model, false).await?;
    Ok(())
}

/// Insert only if missing when `force` is false; always overwrite when force.
async fn upsert_setting(pool: &PgPool, key: &str, value: &str, force: bool) -> AppResult<()> {
    if force {
        sqlx::query(
            r#"
            INSERT INTO app_settings (key, value, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            INSERT INTO app_settings (key, value, updated_at)
            VALUES ($1, $2, NOW())
            ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn load_runtime(pool: &PgPool, fallback: &Config) -> AppResult<LlmRuntime> {
    let base = get_setting(pool, "llm_api_base")
        .await?
        .or_else(|| fallback.llm_api_base.clone())
        .filter(|s| !s.is_empty());
    let key = get_setting(pool, "llm_api_key")
        .await?
        .or_else(|| fallback.llm_api_key.clone())
        .filter(|s| !s.is_empty());
    let model = get_setting(pool, "llm_model")
        .await?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.llm_model.clone());
    Ok(LlmRuntime::from_parts(base, key, model))
}

pub async fn get_setting(pool: &PgPool, key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn save_ai_config(
    pool: &PgPool,
    runtime: &RwLock<LlmRuntime>,
    api_base: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    clear_key: bool,
) -> AppResult<AiConfigView> {
    let current = runtime.read().await.clone();
    let new_base = api_base
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or(current.api_base.clone());
    let new_key = if clear_key {
        None
    } else if let Some(k) = api_key {
        let t = k.trim().to_string();
        if t.is_empty() || t.contains('…') || t.contains('*') {
            current.api_key.clone()
        } else {
            Some(t)
        }
    } else {
        current.api_key.clone()
    };
    let new_model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(current.model);

    upsert_setting(
        pool,
        "llm_api_base",
        new_base.as_deref().unwrap_or(""),
        true,
    )
    .await?;
    upsert_setting(
        pool,
        "llm_api_key",
        new_key.as_deref().unwrap_or(""),
        true,
    )
    .await?;
    upsert_setting(pool, "llm_model", &new_model, true).await?;

    let next = LlmRuntime::from_parts(new_base, new_key, new_model);
    let view = next.view();
    *runtime.write().await = next;
    Ok(view)
}

pub async fn record_usage(
    pool: &PgPool,
    source: &str,
    model: &str,
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    latency_ms: i64,
    success: bool,
    error_message: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO llm_usage_logs
          (source, model, prompt_tokens, completion_tokens, total_tokens, latency_ms, success, error_message)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(source)
    .bind(model)
    .bind(prompt_tokens)
    .bind(completion_tokens)
    .bind(total_tokens)
    .bind(latency_ms as i32)
    .bind(success)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}
