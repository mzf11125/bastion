use bastion_sidecar::{
    audit::AuditLogger,
    build_app,
    grond_oracle::GrondOracle,
    policy::Policy,
    program_client::OnChainClient,
    simulation::{HeliusSimulator, Simulate},
};
use std::env;
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

    let on_chain_enabled = env::var("BASTION_ON_CHAIN").is_ok();
    let on_chain = if on_chain_enabled {
        let rpc_url = env::var("SOLANA_RPC_URL")
            .unwrap_or_else(|_| "https://api.devnet.solana.com".to_string());
        let keypair_path = env::var("BASTION_KEYPAIR_PATH")
            .expect("BASTION_KEYPAIR_PATH required when BASTION_ON_CHAIN is set");
        OnChainClient::new(rpc_url, keypair_path, true).expect("create on-chain client")
    } else {
        eprintln!("[bastion] On-chain audit logging disabled (set BASTION_ON_CHAIN to enable)");
        OnChainClient::disabled()
    };

    let grond_oracle = match env::var("GROND_API_URL") {
        Ok(url) if !url.is_empty() => {
            eprintln!("[bastion] GrondOSINT oracle enabled: {url}");
            GrondOracle::new(url, reqwest::Client::new())
        }
        _ => {
            eprintln!("[bastion] GrondOSINT oracle disabled (set GROND_API_URL to enable)");
            GrondOracle::disabled()
        }
    };

    let app = build_app(policy, simulator, logger, on_chain, grond_oracle);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("bind to port 3000");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}
