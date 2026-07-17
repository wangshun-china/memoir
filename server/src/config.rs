use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub listen_addr: String,
    pub jwt_secret: String,
    /// When true, enables POST /auth/dev-login for CI/local API tests only.
    pub allow_dev_login: bool,
    /// Ops recovery key for admin forgot-password reset (not the login password).
    pub admin_recovery_secret: Option<String>,
    pub wechat_app_id: Option<String>,
    pub wechat_app_secret: Option<String>,
    pub llm_api_base: Option<String>,
    pub llm_api_key: Option<String>,
    pub llm_model: String,
    pub admin_static_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let allow_dev_login = matches!(
            env::var("ALLOW_DEV_LOGIN").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
        );
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://memoir:memoir@127.0.0.1:5432/memoir".into()),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-only-change-me-jwt-secret".into()),
            allow_dev_login,
            admin_recovery_secret: env::var("ADMIN_RECOVERY_SECRET")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            wechat_app_id: env::var("WECHAT_APP_ID").ok().filter(|s| !s.is_empty()),
            wechat_app_secret: env::var("WECHAT_APP_SECRET").ok().filter(|s| !s.is_empty()),
            llm_api_base: env::var("LLM_API_BASE").ok().filter(|s| !s.is_empty()),
            llm_api_key: env::var("LLM_API_KEY").ok().filter(|s| !s.is_empty()),
            llm_model: env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
            admin_static_dir: env::var("ADMIN_STATIC_DIR").unwrap_or_else(|_| "admin/dist".into()),
        })
    }

    pub fn has_llm(&self) -> bool {
        self.llm_api_key.is_some() && self.llm_api_base.is_some()
    }
}
