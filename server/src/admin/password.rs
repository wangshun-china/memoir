use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand_core::OsRng;

use crate::error::{AppError, AppResult};

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Other(anyhow::anyhow!("password hash failed: {e}")))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> AppResult<bool> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| AppError::Other(anyhow::anyhow!("invalid password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn validate_username(username: &str) -> AppResult<()> {
    let u = username.trim();
    if u.len() < 3 || u.len() > 32 {
        return Err(AppError::BadRequest(
            "用户名长度需在 3—32 个字符之间".into(),
        ));
    }
    if !u
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest(
            "用户名仅允许字母、数字、下划线、短横线".into(),
        ));
    }
    Ok(())
}

pub fn validate_password(password: &str) -> AppResult<()> {
    if password.len() < 8 {
        return Err(AppError::BadRequest("密码至少 8 位".into()));
    }
    if password.len() > 128 {
        return Err(AppError::BadRequest("密码过长".into()));
    }
    Ok(())
}
