use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::jwt::issue_token;
use super::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

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
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: Uuid,
    pub nickname: String,
    pub avatar_url: Option<String>,
    pub wechat_openid: Option<String>,
    pub created_at: DateTime<Utc>,
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
        .route("/auth/refresh", post(refresh_stub))
        .route("/me", get(get_me).patch(update_profile))
}

/// Dev-only login. Disabled unless ALLOW_DEV_LOGIN=1 (CI / local API tests).
/// Miniprogram must never call this — use WeChat code login.
async fn dev_login(
    State(state): State<AppState>,
    Json(body): Json<DevLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    if !state.config.allow_dev_login {
        return Err(AppError::Forbidden);
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
        nickname: user.nickname,
        avatar_url: user.avatar_url,
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
        nickname: user.nickname,
        avatar_url: user.avatar_url,
    }))
}

async fn refresh_stub() -> AppResult<Json<serde_json::Value>> {
    Err(AppError::BadRequest(
        "请重新调用 /auth/wechat 获取 token".into(),
    ))
}

async fn get_me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<UserProfile>> {
    let row = sqlx::query_as::<_, UserProfile>(
        r#"
        SELECT id, nickname, avatar_url, wechat_openid, created_at
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
        RETURNING id, nickname, avatar_url, wechat_openid, created_at
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
        RETURNING id, nickname, avatar_url
        "#,
    )
    .bind(openid)
    .bind(nickname)
    .bind(avatar_url)
    .fetch_one(&state.pool)
    .await?;
    Ok(row)
}

async fn exchange_wechat_code(app_id: &str, app_secret: &str, code: &str) -> AppResult<String> {
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
