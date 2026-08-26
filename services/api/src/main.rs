use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use api::aggregate::Registry;
use api::routes::build_router;
use api::state::{AppState, SharedState};
use api::store::MemoryStore;

fn port_from_env() -> u16 {
    std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080)
}

#[tokio::main]
async fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();

    let config = (
        port_from_env(),
        std::env::var("APP_ENV").unwrap_or_else(|_| "development".into()),
    );
    let plaid = api::aggregate::plaid::PlaidClient::from_env();
    if plaid.is_some() {
        tracing::info!("plaid provider configured");
    }
    let providers = Registry { plaid };
    let state: SharedState = Arc::new(AppState {
        store: MemoryStore::new(),
        env: config.1.clone(),
        started: std::time::Instant::now(),
        providers,
    });

    let app = build_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.0));
    tracing::info!(env = %config.1, port = config.0, "starting ultimate-finance-api");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    // Give in-flight requests a beat before teardown.
    tokio::time::sleep(Duration::from_millis(100)).await;
    tracing::info!("shutdown complete");
}
