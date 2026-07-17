use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// User UUID string, or "admin" for admin console.
    pub sub: String,
    /// "user" | "admin"
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

pub fn issue_token(subject: &str, role: &str, secret: &str, ttl_hours: i64) -> AppResult<String> {
    let now = Utc::now();
    let claims = Claims {
        sub: subject.to_string(),
        role: role.to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::hours(ttl_hours)).timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn verify_token(token: &str, secret: &str) -> AppResult<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
