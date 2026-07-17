use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use crate::auth::{issue_token, AdminAuth};
use crate::error::{AppError, AppResult};
use crate::llm::client::ChatMessage;
use crate::settings::{record_usage, save_ai_config};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/login", post(admin_login))
        .route("/admin/overview", get(overview))
        .route("/admin/users", get(list_users))
        .route("/admin/memoirs", get(list_memoirs))
        .route("/admin/ai-config", get(get_ai_config).put(put_ai_config))
        .route("/admin/ai-config/test", post(test_ai))
        .route("/admin/ai-usage", get(ai_usage))
}

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AdminLoginResponse {
    pub token: String,
    pub role: String,
}

async fn admin_login(
    State(state): State<AppState>,
    Json(body): Json<AdminLoginRequest>,
) -> AppResult<Json<AdminLoginResponse>> {
    if body.password.is_empty() || body.password != state.config.admin_password {
        return Err(AppError::Unauthorized);
    }
    let token = issue_token("admin", "admin", &state.config.jwt_secret, 12)?;
    Ok(Json(AdminLoginResponse {
        token,
        role: "admin".into(),
    }))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UserAdminRow {
    id: Uuid,
    wechat_openid: Option<String>,
    nickname: String,
    role: String,
    memoir_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct MemoirAdminRow {
    id: Uuid,
    title: String,
    subject_name: String,
    status: String,
    creator_user_id: Uuid,
    creator_nickname: String,
    chapter_count: i64,
    message_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct Overview {
    users: i64,
    memoirs: i64,
    interview_sessions: i64,
    interview_messages: i64,
    llm_calls: i64,
    llm_tokens: i64,
    llm_success_rate: f64,
    ai: crate::settings::AiConfigView,
}

async fn overview(
    State(state): State<AppState>,
    _admin: AdminAuth,
) -> AppResult<Json<Overview>> {
    let users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await?;
    let memoirs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memoirs")
        .fetch_one(&state.pool)
        .await?;
    let sessions: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM interview_sessions")
        .fetch_one(&state.pool)
        .await?;
    let messages: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM interview_messages")
        .fetch_one(&state.pool)
        .await?;
    let llm_calls: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM llm_usage_logs")
        .fetch_one(&state.pool)
        .await?;
    let llm_tokens: (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(total_tokens), 0) FROM llm_usage_logs")
            .fetch_one(&state.pool)
            .await?;
    let llm_ok: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM llm_usage_logs WHERE success = TRUE")
            .fetch_one(&state.pool)
            .await?;
    let rate = if llm_calls.0 == 0 {
        1.0
    } else {
        llm_ok.0 as f64 / llm_calls.0 as f64
    };
    let ai = state.llm_runtime.read().await.view();
    Ok(Json(Overview {
        users: users.0,
        memoirs: memoirs.0,
        interview_sessions: sessions.0,
        interview_messages: messages.0,
        llm_calls: llm_calls.0,
        llm_tokens: llm_tokens.0,
        llm_success_rate: rate,
        ai,
    }))
}

async fn list_users(
    State(state): State<AppState>,
    _admin: AdminAuth,
) -> AppResult<Json<Vec<UserAdminRow>>> {
    let rows = sqlx::query_as::<_, UserAdminRow>(
        r#"
        SELECT u.id, u.wechat_openid, u.nickname, u.role,
               (SELECT COUNT(*) FROM memoirs m WHERE m.creator_user_id = u.id) AS memoir_count,
               u.created_at, u.updated_at
        FROM users u
        ORDER BY u.created_at DESC
        LIMIT 500
        "#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn list_memoirs(
    State(state): State<AppState>,
    _admin: AdminAuth,
) -> AppResult<Json<Vec<MemoirAdminRow>>> {
    let rows = sqlx::query_as::<_, MemoirAdminRow>(
        r#"
        SELECT m.id, m.title, m.subject_name, m.status, m.creator_user_id,
               COALESCE(u.nickname, '') AS creator_nickname,
               (SELECT COUNT(*) FROM chapters c WHERE c.memoir_id = m.id) AS chapter_count,
               (
                 SELECT COUNT(*) FROM interview_messages im
                 JOIN interview_sessions s ON s.id = im.session_id
                 WHERE s.memoir_id = m.id
               ) AS message_count,
               m.created_at, m.updated_at
        FROM memoirs m
        LEFT JOIN users u ON u.id = m.creator_user_id
        ORDER BY m.created_at DESC
        LIMIT 500
        "#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn get_ai_config(
    State(state): State<AppState>,
    _admin: AdminAuth,
) -> AppResult<Json<crate::settings::AiConfigView>> {
    Ok(Json(state.llm_runtime.read().await.view()))
}

#[derive(Debug, Deserialize)]
pub struct PutAiConfigRequest {
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub clear_api_key: Option<bool>,
}

async fn put_ai_config(
    State(state): State<AppState>,
    _admin: AdminAuth,
    Json(body): Json<PutAiConfigRequest>,
) -> AppResult<Json<crate::settings::AiConfigView>> {
    let view = save_ai_config(
        &state.pool,
        &state.llm_runtime,
        body.api_base,
        body.api_key,
        body.model,
        body.clear_api_key.unwrap_or(false),
    )
    .await?;
    Ok(Json(view))
}

#[derive(Debug, Deserialize)]
pub struct TestAiRequest {
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestAiResponse {
    pub ok: bool,
    pub reply: String,
    pub model: String,
    pub mode: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub latency_ms: i64,
    pub error: Option<String>,
}

async fn test_ai(
    State(state): State<AppState>,
    _admin: AdminAuth,
    Json(body): Json<TestAiRequest>,
) -> AppResult<Json<TestAiResponse>> {
    let prompt = body
        .prompt
        .unwrap_or_else(|| "请用一句话自我介绍你是回忆录采访助手。".into());
    let runtime = state.llm_runtime.read().await;
    let client = runtime.client.clone();
    let mode = client.kind().to_string();
    drop(runtime);

    let messages = vec![
        ChatMessage {
            role: "system".into(),
            content: "你是回忆录采访助手，回复要简短。".into(),
        },
        ChatMessage {
            role: "user".into(),
            content: prompt,
        },
    ];

    match client.complete(&messages).await {
        Ok(comp) => {
            let _ = record_usage(
                &state.pool,
                "admin_test",
                &comp.model,
                comp.prompt_tokens,
                comp.completion_tokens,
                comp.total_tokens,
                comp.latency_ms,
                true,
                None,
            )
            .await;
            Ok(Json(TestAiResponse {
                ok: true,
                reply: comp.content,
                model: comp.model,
                mode,
                prompt_tokens: comp.prompt_tokens,
                completion_tokens: comp.completion_tokens,
                total_tokens: comp.total_tokens,
                latency_ms: comp.latency_ms,
                error: None,
            }))
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = record_usage(
                &state.pool,
                "admin_test",
                client.model_name(),
                0,
                0,
                0,
                0,
                false,
                Some(&msg),
            )
            .await;
            Ok(Json(TestAiResponse {
                ok: false,
                reply: String::new(),
                model: client.model_name().to_string(),
                mode,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                latency_ms: Instant::now().elapsed().as_millis() as i64,
                error: Some(msg),
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UsageRow {
    id: Uuid,
    source: String,
    model: String,
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    latency_ms: i32,
    success: bool,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct UsageResponse {
    summary: UsageSummary,
    recent: Vec<UsageRow>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct UsageSummary {
    calls: i64,
    success_calls: i64,
    total_tokens: i64,
    prompt_tokens: i64,
    completion_tokens: i64,
    avg_latency_ms: f64,
}

async fn ai_usage(
    State(state): State<AppState>,
    _admin: AdminAuth,
    Query(q): Query<UsageQuery>,
) -> AppResult<Json<UsageResponse>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let summary = sqlx::query_as::<_, UsageSummary>(
        r#"
        SELECT
          COUNT(*)::bigint AS calls,
          COUNT(*) FILTER (WHERE success)::bigint AS success_calls,
          COALESCE(SUM(total_tokens), 0)::bigint AS total_tokens,
          COALESCE(SUM(prompt_tokens), 0)::bigint AS prompt_tokens,
          COALESCE(SUM(completion_tokens), 0)::bigint AS completion_tokens,
          COALESCE(AVG(latency_ms), 0)::float8 AS avg_latency_ms
        FROM llm_usage_logs
        "#,
    )
    .fetch_one(&state.pool)
    .await?;

    let recent = sqlx::query_as::<_, UsageRow>(
        r#"
        SELECT id, source, model, prompt_tokens, completion_tokens, total_tokens,
               latency_ms, success, error_message, created_at
        FROM llm_usage_logs
        ORDER BY created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(UsageResponse { summary, recent }))
}
