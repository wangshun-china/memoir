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
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = extract_claims(parts, state)?;
        if claims.role != "user" && claims.role != "admin" {
            return Err(AppError::Unauthorized);
        }
        // Admin token is not a user JWT for resource ownership paths.
        if claims.role == "admin" {
            return Err(AppError::Unauthorized);
        }
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;
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
        if claims.role != "admin" {
            return Err(AppError::Forbidden);
        }
        Ok(AdminAuth {
            subject: claims.sub,
        })
    }
}

fn extract_claims(parts: &Parts, state: &AppState) -> Result<super::jwt::Claims, AppError> {
    let auth = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let token = auth.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;

    verify_token(token, &state.config.jwt_secret)
}
