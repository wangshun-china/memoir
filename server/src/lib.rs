pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod interviews;
pub mod llm;
pub mod memoirs;
pub mod state;

use axum::routing::get;
use axum::{Json, Router};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::llm::client::build_llm_client;
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .merge(auth::router())
        .merge(memoirs::router())
        .merge(interviews::router());

    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "memoir-server" }))
}

/// Build app from env (DATABASE_URL, etc.) for integration tests and embedding.
pub async fn app_from_env() -> anyhow::Result<(Router, AppState)> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    let llm = build_llm_client(&config);
    let state = AppState { pool, config, llm };
    let router = build_router(state.clone());
    Ok((router, state))
}
