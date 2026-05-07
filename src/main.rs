use sentinel::{
    audit::AuditLogger,
    build_app,
    policy::Policy,
    simulation::{HeliusSimulator, Simulate},
};
use std::fs;
use std::sync::Arc;
use tokio::signal;

async fn shutdown_signal() {
    let _ = signal::ctrl_c().await;
    eprintln!("shutdown signal received");
}

#[tokio::main]
async fn main() {
    let config_text = fs::read_to_string("config.toml").expect("read config.toml");
    let policy: Policy = toml::from_str(&config_text).expect("parse config.toml");
    let simulator: Arc<dyn Simulate + Send + Sync> =
        Arc::new(HeliusSimulator::new().expect("create Helius simulator"));
    let logger = Arc::new(AuditLogger::new("audit_logs").expect("create audit logger"));
    let app = build_app(policy, simulator, logger);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind to port 3000");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}
