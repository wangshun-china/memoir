use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

use memoir_server::build_router;
use memoir_server::config::Config;
use memoir_server::db;
use memoir_server::llm::client::build_llm_client;
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

    let llm = build_llm_client(&config);
    let state = AppState {
        pool,
        config: config.clone(),
        llm,
    };

    let app = build_router(state);

    let addr: SocketAddr = config.listen_addr.parse()?;
    tracing::info!(%addr, "memoir-server listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
