use crate::config::Config;
use crate::llm::client::LlmClient;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub llm: Arc<dyn LlmClient>,
}
