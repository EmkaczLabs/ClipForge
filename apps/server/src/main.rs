use clipforge_server::build_app;
use clipforge_server::config::ServerConfig;

#[tokio::main]
async fn main() {
    clipforge_common::init_tracing("info");

    let config = ServerConfig::from_env().unwrap_or_else(|e| {
        eprintln!("configuration error: {e}");
        std::process::exit(1);
    });
    let bind_addr = config.bind_addr.clone();

    let app = build_app(config).await.unwrap_or_else(|e| {
        eprintln!("failed to initialize server: {e}");
        std::process::exit(1);
    });

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("bind server port");
    tracing::info!("clipforge-server listening on {bind_addr}");
    axum::serve(listener, app).await.expect("serve");
}
