use buzz_account::{db, http, Config};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // rustls needs one provider chosen explicitly (both ring and aws-lc-rs are
    // compiled in transitively through the workspace). Mirrors buzz-admin.
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        anyhow::bail!("failed to install rustls crypto provider");
    }

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env()?;
    tracing::info!(?config, "starting buzz-account");

    let pool = db::connect_and_migrate(&config.database_url).await?;
    let bind_addr = config.bind_addr;
    let app = http::router(http::AppState::new(config, pool)?);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %bind_addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
