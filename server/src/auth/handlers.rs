use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::jwt::issue_token;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DevLoginRequest {
    /// Optional nickname for the mock user.
    pub nickname: Option<String>,
    /// Optional stable openid for repeatable local testing.
    pub openid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WechatLoginRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: Uuid,
    pub nickname: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/dev-login", post(dev_login))
        .route("/auth/wechat", post(wechat_login))
        .route("/auth/refresh", post(refresh_stub))
}

async fn dev_login(
    State(state): State<AppState>,
    Json(body): Json<DevLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    let openid = body
        .openid
        .unwrap_or_else(|| format!("dev-{}", Uuid::new_v4()));
    let nickname = body.nickname.unwrap_or_else(|| "开发测试用户".to_string());

    let user = upsert_user(&state, &openid, &nickname).await?;
    let token = issue_token(user.id, &state.config.jwt_secret, 72)?;

    Ok(Json(AuthResponse {
        token,
        user_id: user.id,
        nickname: user.nickname,
    }))
}

/// WeChat code exchange. Without real credentials, treats `code` as a stable openid suffix.
async fn wechat_login(
    State(state): State<AppState>,
    Json(body): Json<WechatLoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    if body.code.trim().is_empty() {
        return Err(AppError::BadRequest("code is required".into()));
    }

    let openid = if let (Some(app_id), Some(app_secret)) = (
        state.config.wechat_app_id.as_ref(),
        state.config.wechat_app_secret.as_ref(),
    ) {
        exchange_wechat_code(app_id, app_secret, &body.code).await?
    } else {
        // Documented Stage-1 mock: prefix wx-mock- for local/dev without AppSecret.
        format!("wx-mock-{}", body.code)
    };

    let user = upsert_user(&state, &openid, "微信用户").await?;
    let token = issue_token(user.id, &state.config.jwt_secret, 72)?;

    Ok(Json(AuthResponse {
        token,
        user_id: user.id,
        nickname: user.nickname,
    }))
}

async fn refresh_stub() -> AppResult<Json<serde_json::Value>> {
    Err(AppError::BadRequest(
        "use /auth/dev-login or /auth/wechat to obtain a new token in Stage 1".into(),
    ))
}

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    nickname: String,
}

async fn upsert_user(state: &AppState, openid: &str, nickname: &str) -> AppResult<UserRow> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (wechat_openid, nickname)
        VALUES ($1, $2)
        ON CONFLICT (wechat_openid) DO UPDATE
          SET nickname = CASE
                WHEN EXCLUDED.nickname <> '' AND users.nickname = '微信用户'
                THEN EXCLUDED.nickname
                ELSE users.nickname
              END,
              updated_at = NOW()
        RETURNING id, nickname
        "#,
    )
    .bind(openid)
    .bind(nickname)
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
        "wechat login failed: {errcode} {errmsg}"
    )))
}
