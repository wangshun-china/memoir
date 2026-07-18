use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use super::password::{hash_password, validate_password, validate_username, verify_password};
use crate::auth::{issue_token, AdminAuth};
use crate::error::{AppError, AppResult};
use crate::llm::client::ChatMessage;
use crate::settings::{record_usage, save_ai_config};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/setup-status", get(setup_status))
        .route("/admin/setup", post(admin_setup))
        .route("/admin/login", post(admin_login))
        .route("/admin/reset-password", post(reset_password))
        .route("/admin/change-password", post(change_password))
        .route("/admin/me", get(admin_me))
        .route("/admin/overview", get(overview))
        .route("/admin/users", get(list_users))
        .route("/admin/memoirs", get(list_memoirs))
        .route("/admin/ai-config", get(get_ai_config).put(put_ai_config))
        .route("/admin/ai-config/test", post(test_ai))
        .route("/admin/ai-usage", get(ai_usage))
}

#[derive(Debug, Serialize)]
struct SetupStatus {
    /// true when no admin account exists yet — UI must force create-admin flow.
    needs_setup: bool,
    admin_count: i64,
    /// true when ADMIN_RECOVERY_SECRET is configured — forgot-password UI can show.
    recovery_enabled: bool,
}

async fn setup_status(State(state): State<AppState>) -> AppResult<Json<SetupStatus>> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admin_accounts")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(SetupStatus {
        needs_setup: count == 0,
        admin_count: count,
        recovery_enabled: state
            .config
            .admin_recovery_secret
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
    }))
}

#[derive(Debug, Deserialize)]
pub struct AdminSetupRequest {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdminLoginResponse {
    pub token: String,
    pub role: String,
    pub admin_id: Uuid,
    pub username: String,
    pub display_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AdminRow {
    id: Uuid,
    username: String,
    password_hash: String,
    display_name: String,
}

/// Create the first admin account. Fails if any admin already exists.
async fn admin_setup(
    State(state): State<AppState>,
    Json(body): Json<AdminSetupRequest>,
) -> AppResult<Json<AdminLoginResponse>> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admin_accounts")
        .fetch_one(&state.pool)
        .await?;
    if count > 0 {
        return Err(AppError::Conflict(
            "管理员已创建，请使用账号密码登录".into(),
        ));
    }

    validate_username(&body.username)?;
    validate_password(&body.password)?;
    let username = body.username.trim().to_string();
    let display_name = body
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(username.as_str())
        .to_string();
    let password_hash = hash_password(&body.password)?;

    let row = sqlx::query_as::<_, AdminRow>(
        r#"
        INSERT INTO admin_accounts (username, password_hash, display_name)
        VALUES ($1, $2, $3)
        RETURNING id, username, password_hash, display_name
        "#,
    )
    .bind(&username)
    .bind(&password_hash)
    .bind(&display_name)
    .fetch_one(&state.pool)
    .await?;

    let token = issue_token(&row.id.to_string(), "admin", &state.config.jwt_secret, 12)?;
    Ok(Json(AdminLoginResponse {
        token,
        role: "admin".into(),
        admin_id: row.id,
        username: row.username,
        display_name: row.display_name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

async fn admin_login(
    State(state): State<AppState>,
    Json(body): Json<AdminLoginRequest>,
) -> AppResult<Json<AdminLoginResponse>> {
    let username = body.username.trim();
    if username.is_empty() || body.password.is_empty() {
        return Err(AppError::Unauthorized("请输入用户名和密码".into()));
    }

    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admin_accounts")
        .fetch_one(&state.pool)
        .await?;
    if count == 0 {
        return Err(AppError::BadRequest(
            "尚未创建管理员，请先完成初始化".into(),
        ));
    }

    // Case-sensitive exact username match against admin_accounts.username
    let row = sqlx::query_as::<_, AdminRow>(
        r#"
        SELECT id, username, password_hash, display_name
        FROM admin_accounts
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".into()))?;

    // Argon2id verify: hash stored at registration, never plain text
    if !verify_password(&body.password, &row.password_hash)? {
        return Err(AppError::Unauthorized("用户名或密码错误".into()));
    }

    sqlx::query("UPDATE admin_accounts SET last_login_at = NOW() WHERE id = $1")
        .bind(row.id)
        .execute(&state.pool)
        .await?;

    let token = issue_token(&row.id.to_string(), "admin", &state.config.jwt_secret, 12)?;
    Ok(Json(AdminLoginResponse {
        token,
        role: "admin".into(),
        admin_id: row.id,
        username: row.username,
        display_name: row.display_name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub username: String,
    /// Must match server env ADMIN_RECOVERY_SECRET (ops recovery key).
    pub recovery_secret: String,
    pub new_password: String,
}

/// Forgot-password reset: proves ops identity via ADMIN_RECOVERY_SECRET, then sets new Argon2 hash.
async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let expected = state
        .config
        .admin_recovery_secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            AppError::BadRequest(
                "服务器未配置 ADMIN_RECOVERY_SECRET，无法在线重置。请联系运维在环境变量中设置恢复密钥。"
                    .into(),
            )
        })?;

    if body.recovery_secret.trim() != expected {
        return Err(AppError::Unauthorized("恢复密钥错误".into()));
    }

    validate_username(&body.username)?;
    validate_password(&body.new_password)?;
    let username = body.username.trim();

    let row = sqlx::query_as::<_, AdminRow>(
        r#"
        SELECT id, username, password_hash, display_name
        FROM admin_accounts WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("该用户名不存在".into()))?;

    let new_hash = hash_password(&body.new_password)?;
    sqlx::query(
        r#"
        UPDATE admin_accounts
        SET password_hash = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(&new_hash)
    .bind(row.id)
    .execute(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "username": row.username,
        "message": "密码已重置，请使用新密码登录"
    })))
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    admin: AdminAuth,
    Json(body): Json<ChangePasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let admin_id =
        Uuid::parse_str(&admin.subject).map_err(|_| AppError::Unauthorized("登录已失效".into()))?;
    validate_password(&body.new_password)?;

    let row = sqlx::query_as::<_, AdminRow>(
        r#"
        SELECT id, username, password_hash, display_name
        FROM admin_accounts WHERE id = $1
        "#,
    )
    .bind(admin_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("登录已失效".into()))?;

    if !verify_password(&body.current_password, &row.password_hash)? {
        return Err(AppError::Unauthorized("当前密码不正确".into()));
    }

    let new_hash = hash_password(&body.new_password)?;
    sqlx::query(
        r#"
        UPDATE admin_accounts
        SET password_hash = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(&new_hash)
    .bind(admin_id)
    .execute(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Serialize)]
struct AdminMe {
    admin_id: Uuid,
    username: String,
    display_name: String,
}

async fn admin_me(State(state): State<AppState>, admin: AdminAuth) -> AppResult<Json<AdminMe>> {
    let admin_id =
        Uuid::parse_str(&admin.subject).map_err(|_| AppError::Unauthorized("登录已失效".into()))?;
    let row = sqlx::query_as::<_, AdminRow>(
        r#"
        SELECT id, username, password_hash, display_name
        FROM admin_accounts WHERE id = $1
        "#,
    )
    .bind(admin_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("登录已失效".into()))?;

    Ok(Json(AdminMe {
        admin_id: row.id,
        username: row.username,
        display_name: row.display_name,
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

async fn overview(State(state): State<AppState>, _admin: AdminAuth) -> AppResult<Json<Overview>> {
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
    let llm_ok: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM llm_usage_logs WHERE success = TRUE")
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

    match client
        .complete_with(&messages, crate::llm::client::CompleteOptions::admin_test())
        .await
    {
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
