use crate::config::Config;
use crate::settings::LlmRuntime;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub llm_runtime: Arc<RwLock<LlmRuntime>>,
}
