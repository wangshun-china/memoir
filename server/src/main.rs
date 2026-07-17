use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use memoir_server::build_router;
use memoir_server::config::Config;
use memoir_server::db;
use memoir_server::settings::{load_runtime, seed_settings_from_env};
use memoir_server::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    seed_settings_from_env(&pool, &config).await?;
    let runtime = load_runtime(&pool, &config).await?;

    let state = AppState {
        pool,
        config: config.clone(),
        llm_runtime: Arc::new(RwLock::new(runtime)),
    };

    let app = build_router(state);

    let addr: SocketAddr = config.listen_addr.parse()?;
    tracing::info!(%addr, "memoir-server listening");
    tracing::info!(
        admin = %format!("http://{addr}/admin/"),
        "admin console"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
