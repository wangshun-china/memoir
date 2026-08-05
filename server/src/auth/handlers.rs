use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::jwt::issue_token;
use super::AuthUser;
use crate::admin::password::{hash_password, validate_password, validate_username, verify_password};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Trial / stage: this username always gets is_admin = true.
const ADMIN_USERNAME: &str = "wangshun";

#[derive(Debug, Deserialize)]
pub struct DevLoginRequest {
    pub nickname: Option<String>,
    pub openid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WechatLoginRequest {
    pub code: String,
    /// Optional nickname from miniprogram (user chose after login).
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: Uuid,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub username: Option<String>,
    pub is_admin: bool,
    /// true when this call created a new account (password login auto-register).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub registered: bool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: Uuid,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub wechat_openid: Option<String>,
    pub username: Option<String>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PasswordLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub username: String,
    /// Trial: must be `wangshun`.
    pub recovery_key: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub nickname: Option<String>,
    pub avatar_url: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/dev-login", post(dev_login))
        .route("/auth/wechat", post(wechat_login))
        .route("/auth/password", post(password_login))
        .route("/auth/reset-password", post(reset_password))
        .route("/auth/refresh", post(refresh_stub))
        .route("/me", get(get_me).patch(update_profile))
}

/// Trial recovery key for forgot-password (miniprogram). Same string as reserved admin username.
const RECOVERY_KEY: &str = "wangshun";

/// Dev-only login. Disabled unless ALLOW_DEV_LOGIN=1 (CI / local API tests).
/// Miniprogram must never call this — use WeChat code login.
async fn dev_login(
    State(state): State<AppState>,
    Json(body): Json<DevLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    if !state.config.allow_dev_login {
        return Err(AppError::Forbidden(
            "开发登录未启用（ALLOW_DEV_LOGIN）".into(),
        ));
    }
    let openid = body
        .openid
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("openid is required when ALLOW_DEV_LOGIN=1".into()))?;
    let nickname = body
        .nickname
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "开发测试用户".to_string());

    let user = upsert_user(&state, &openid, &nickname, None).await?;
    let token = issue_token(&user.id.to_string(), "user", &state.config.jwt_secret, 72)?;

    Ok(Json(AuthResponse {
        token,
        user_id: user.id,
        nickname: user.nickname.clone(),
        avatar_url: user.avatar_url.clone(),
        username: user.username,
        is_admin: user.is_admin,
        registered: false,
    }))
}

/// Real WeChat mini-program login: exchange js_code via WeChat API.
/// Requires WECHAT_APP_ID + WECHAT_APP_SECRET. No mock openid.
async fn wechat_login(
    State(state): State<AppState>,
    Json(body): Json<WechatLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    if body.code.trim().is_empty() {
        return Err(AppError::BadRequest("code is required".into()));
    }

    let (app_id, app_secret) = match (
        state.config.wechat_app_id.as_ref(),
        state.config.wechat_app_secret.as_ref(),
    ) {
        (Some(a), Some(s)) => (a, s),
        _ => {
            return Err(AppError::BadRequest(
                "服务器未配置微信登录（WECHAT_APP_ID / WECHAT_APP_SECRET）".into(),
            ));
        }
    };

    let openid = exchange_wechat_code(app_id, app_secret, &body.code).await?;
    let nickname = body
        .nickname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("微信用户");
    let avatar = body
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let user = upsert_user(&state, &openid, nickname, avatar.as_deref()).await?;
    let token = issue_token(&user.id.to_string(), "user", &state.config.jwt_secret, 72)?;

    Ok(Json(AuthResponse {
        token,
        user_id: user.id,
        nickname: user.nickname.clone(),
        avatar_url: user.avatar_url.clone(),
        username: user.username,
        is_admin: user.is_admin,
        registered: false,
    }))
}

/// Password login: if username missing → auto register; if exists → verify password.
/// Username `wangshun` (case-insensitive) is always granted is_admin.
async fn password_login(
    State(state): State<AppState>,
    Json(body): Json<PasswordLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    let username = body.username.trim().to_string();
    let password = body.password;
    validate_username(&username)?;
    validate_password(&password)?;

    let is_admin_name = username.eq_ignore_ascii_case(ADMIN_USERNAME);

    let existing = sqlx::query_as::<_, PasswordUserRow>(
        r#"
        SELECT id, nickname, avatar_url, username, password_hash, is_admin
        FROM users
        WHERE username = $1
        "#,
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await?;

    let (user, registered) = if let Some(row) = existing {
        let hash = row.password_hash.as_deref().unwrap_or("");
        if hash.is_empty() || !verify_password(&password, hash)? {
            return Err(AppError::Unauthorized("账号或密码错误".into()));
        }
        // Keep admin flag in sync for the reserved username.
        let is_admin = if is_admin_name || row.is_admin {
            if !row.is_admin {
                sqlx::query("UPDATE users SET is_admin = TRUE, updated_at = NOW() WHERE id = $1")
                    .bind(row.id)
                    .execute(&state.pool)
                    .await?;
            }
            true
        } else {
            row.is_admin
        };
        (
            UserRow {
                id: row.id,
                nickname: row.nickname,
                avatar_url: row.avatar_url,
                username: row.username,
                is_admin,
            },
            false,
        )
    } else {
        let hash = hash_password(&password)?;
        let nickname = username.clone();
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            INSERT INTO users (username, password_hash, nickname, is_admin)
            VALUES ($1, $2, $3, $4)
            RETURNING id, nickname, avatar_url, username, is_admin
            "#,
        )
        .bind(&username)
        .bind(&hash)
        .bind(&nickname)
        .bind(is_admin_name)
        .fetch_one(&state.pool)
        .await?;
        (row, true)
    };

    let token = issue_token(&user.id.to_string(), "user", &state.config.jwt_secret, 72)?;
    Ok(Json(AuthResponse {
        token,
        user_id: user.id,
        nickname: user.nickname,
        avatar_url: user.avatar_url,
        username: user.username,
        is_admin: user.is_admin,
        registered,
    }))
}

/// Forgot password: prove recovery_key (`wangshun` for trial), set new password for username.
async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let username = body.username.trim().to_string();
    validate_username(&username)?;
    validate_password(&body.new_password)?;

    if body.recovery_key.trim() != RECOVERY_KEY {
        return Err(AppError::Unauthorized("恢复密钥错误".into()));
    }

    let row = sqlx::query_as::<_, PasswordUserRow>(
        r#"
        SELECT id, nickname, avatar_url, username, password_hash, is_admin
        FROM users
        WHERE username = $1
        "#,
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("该账号不存在".into()))?;

    if row.password_hash.is_none() {
        return Err(AppError::BadRequest(
            "该账号未设置密码（可能是微信登录账号）".into(),
        ));
    }

    let new_hash = hash_password(&body.new_password)?;
    sqlx::query(
        r#"
        UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2
        "#,
    )
    .bind(&new_hash)
    .bind(row.id)
    .execute(&state.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "username": username,
        "message": "密码已重置，请使用新密码登录"
    })))
}

async fn refresh_stub() -> AppResult<Json<serde_json::Value>> {
    Err(AppError::BadRequest(
        "请重新调用 /auth/wechat 或 /auth/password 获取 token".into(),
    ))
}

async fn get_me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<UserProfile>> {
    let row = sqlx::query_as::<_, UserProfile>(
        r#"
        SELECT id, nickname, avatar_url, wechat_openid, username, is_admin, created_at
        FROM users WHERE id = $1
        "#,
    )
    .bind(user.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".into()))?;
    Ok(Json(row))
}

async fn update_profile(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> AppResult<Json<UserProfile>> {
    let nickname = body
        .nickname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let avatar = body
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if nickname.is_none() && avatar.is_none() {
        return Err(AppError::BadRequest("nothing to update".into()));
    }

    let row = sqlx::query_as::<_, UserProfile>(
        r#"
        UPDATE users SET
          nickname = COALESCE($2, nickname),
          avatar_url = COALESCE($3, avatar_url),
          updated_at = NOW()
        WHERE id = $1
        RETURNING id, nickname, avatar_url, wechat_openid, username, is_admin, created_at
        "#,
    )
    .bind(user.user_id)
    .bind(nickname)
    .bind(avatar)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row))
}

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    nickname: String,
    avatar_url: Option<String>,
    username: Option<String>,
    is_admin: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct PasswordUserRow {
    id: Uuid,
    nickname: String,
    avatar_url: Option<String>,
    username: Option<String>,
    password_hash: Option<String>,
    is_admin: bool,
}

async fn upsert_user(
    state: &AppState,
    openid: &str,
    nickname: &str,
    avatar_url: Option<&str>,
) -> AppResult<UserRow> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (wechat_openid, nickname, avatar_url)
        VALUES ($1, $2, $3)
        ON CONFLICT (wechat_openid) DO UPDATE
          SET nickname = CASE
                WHEN EXCLUDED.nickname <> '' AND EXCLUDED.nickname <> '微信用户'
                THEN EXCLUDED.nickname
                ELSE users.nickname
              END,
              avatar_url = COALESCE(EXCLUDED.avatar_url, users.avatar_url),
              updated_at = NOW()
        RETURNING id, nickname, avatar_url, username, is_admin
        "#,
    )
    .bind(openid)
    .bind(nickname)
    .bind(avatar_url)
    .fetch_one(&state.pool)
    .await?;
    Ok(row)
}

pub(crate) async fn exchange_wechat_code(
    app_id: &str,
    app_secret: &str,
    code: &str,
) -> AppResult<String> {
    let url = format!(
        "https://api.weixin.qq.com/sns/jscode2session?appid={app_id}&secret={app_secret}&js_code={code}&grant_type=authorization_code"
    );
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| AppError::Other(e.into()))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| AppError::Other(e.into()))?;

    if let Some(openid) = resp.get("openid").and_then(|v| v.as_str()) {
        return Ok(openid.to_string());
    }
    let errcode = resp.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    let errmsg = resp
        .get("errmsg")
        .and_then(|v| v.as_str())
        .unwrap_or("wechat login failed");
    Err(AppError::BadRequest(format!(
        "微信登录失败: {errcode} {errmsg}"
    )))
}
