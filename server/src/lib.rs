pub mod admin;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod interviews;
pub mod llm;
pub mod memoirs;
pub mod settings;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{header, HeaderValue};
use axum::routing::get;
use axum::{Json, Router};
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::settings::{load_runtime, seed_settings_from_env};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .merge(auth::router())
        .merge(memoirs::router())
        .merge(interviews::router())
        .merge(admin::router());

    let static_dir = PathBuf::from(&state.config.admin_static_dir);
    let index = static_dir.join("index.html");
    // Admin SPA must not be cached — otherwise browsers keep the old login-only page.
    let admin_svc = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        ))
        .service(ServeDir::new(static_dir).not_found_service(ServeFile::new(index)));

    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api)
        .nest_service("/admin", admin_svc)
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
    seed_settings_from_env(&pool, &config).await?;
    let runtime = load_runtime(&pool, &config).await?;
    let state = AppState {
        pool,
        config,
        llm_runtime: Arc::new(RwLock::new(runtime)),
    };
    let router = build_router(state.clone());
    Ok((router, state))
}
