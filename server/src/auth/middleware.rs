use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use super::jwt::verify_token;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AdminAuth {
    pub subject: String,
    /// true when authorized via users.is_admin (miniprogram admin).
    pub via_user_admin: bool,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = extract_claims(parts, state)?;
        if claims.role != "user" && claims.role != "admin" {
            return Err(AppError::Unauthorized("未登录或登录已失效".into()));
        }
        // Admin token is not a user JWT for resource ownership paths.
        if claims.role == "admin" {
            return Err(AppError::Unauthorized("请使用用户身份访问".into()));
        }
        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AppError::Unauthorized("未登录或登录已失效".into()))?;
        Ok(AuthUser { user_id })
    }
}

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = extract_claims(parts, state)?;
        if claims.role == "admin" {
            return Ok(AdminAuth {
                subject: claims.sub,
                via_user_admin: false,
            });
        }
        if claims.role == "user" {
            let user_id = Uuid::parse_str(&claims.sub)
                .map_err(|_| AppError::Forbidden("需要管理员权限".into()))?;
            let is_admin: Option<bool> =
                sqlx::query_scalar("SELECT is_admin FROM users WHERE id = $1")
                    .bind(user_id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(AppError::from)?;
            if is_admin == Some(true) {
                return Ok(AdminAuth {
                    subject: claims.sub,
                    via_user_admin: true,
                });
            }
        }
        Err(AppError::Forbidden("需要管理员权限".into()))
    }
}

fn extract_claims(parts: &Parts, state: &AppState) -> Result<super::jwt::Claims, AppError> {
    let auth = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("未登录或登录已失效".into()))?;

    let token = auth
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("未登录或登录已失效".into()))?;

    verify_token(token, &state.config.jwt_secret)
        .map_err(|_| AppError::Unauthorized("未登录或登录已失效".into()))
}
