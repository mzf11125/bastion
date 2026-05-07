//! Phase 5 — Transaction-battery tests covering DEX swaps, system transfers,
//! stake operations, and edge-case transactions evaluated through the full
//! REST API stack with a mock simulator.

use anyhow::Result;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::Engine as _;
use serde_json::json;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    stake, system_instruction, system_program,
    transaction::Transaction,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

use sentinel::{
    build_app,
    logger::{AuditEntry, AuditLogger, AuditResult, Decision},
    policy::Policy,
    simulation::{Simulate, SimulationResult},
};

// ── Well-known program IDs ───────────────────────────────────────────────

fn jupiter_v6() -> Pubkey {
    Pubkey::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap()
}

fn raydium_amm() -> Pubkey {
    Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8").unwrap()
}

fn token_program() -> Pubkey {
    Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap()
}

// ── Transaction builders ─────────────────────────────────────────────────

fn make_dex_swap_tx(program_id: Pubkey) -> Transaction {
    let payer = Keypair::new();
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
            AccountMeta::new(Pubkey::new_unique(), false),
        ],
        data: vec![0xE5, 0x17, 0xCB, 0x97], // swap discriminator
    };
    Transaction::new_unsigned(Message::new(&[ix], Some(&payer.pubkey())))
}

fn make_transfer_tx(lamports: u64) -> Transaction {
    let payer = Keypair::new();
    Transaction::new_unsigned(Message::new(
        &[system_instruction::transfer(
            &payer.pubkey(),
            &Pubkey::new_unique(),
            lamports,
        )],
        Some(&payer.pubkey()),
    ))
}

fn make_stake_delegate_tx() -> Transaction {
    let authority = Keypair::new();
    let stake_account = Pubkey::new_unique();
    let vote_account = Pubkey::new_unique();
    Transaction::new_unsigned(Message::new(
        &[stake::instruction::delegate_stake(
            &stake_account,
            &authority.pubkey(),
            &vote_account,
        )],
        Some(&authority.pubkey()),
    ))
}

fn make_stake_deactivate_tx() -> Transaction {
    let authority = Keypair::new();
    let stake_account = Pubkey::new_unique();
    Transaction::new_unsigned(Message::new(
        &[stake::instruction::deactivate_stake(
            &stake_account,
            &authority.pubkey(),
        )],
        Some(&authority.pubkey()),
    ))
}

fn make_multi_ix_tx(programs: &[Pubkey]) -> Transaction {
    let payer = Keypair::new();
    let instructions: Vec<Instruction> = programs
        .iter()
        .map(|program_id| Instruction {
            program_id: *program_id,
            accounts: vec![AccountMeta::new(payer.pubkey(), true)],
            data: vec![1, 2, 3],
        })
        .collect();
    Transaction::new_unsigned(Message::new(&instructions, Some(&payer.pubkey())))
}

fn encode_tx(tx: &Transaction) -> String {
    let bytes = bincode::serialize(tx).expect("serialize");
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ── Mock simulator ───────────────────────────────────────────────────────

#[derive(Clone)]
struct ConfigurableSimulator {
    units: u64,
    error: Option<serde_json::Value>,
    balance_changes: HashMap<String, i64>,
}

impl ConfigurableSimulator {
    fn ok(units: u64) -> Self {
        Self {
            units,
            error: None,
            balance_changes: HashMap::new(),
        }
    }

    fn with_drain(mut self, account: &str, drain: u64) -> Self {
        self.balance_changes
            .insert(account.to_string(), -(drain as i64));
        self
    }
}

impl Simulate for ConfigurableSimulator {
    fn simulate_transaction(&self, _tx: &Transaction) -> Result<SimulationResult> {
        Ok(SimulationResult {
            logs: vec!["mock simulation".into()],
            units_consumed: Some(self.units),
            return_data: None,
            error: self.error.clone(),
            balance_changes: self.balance_changes.clone(),
        })
    }
}

fn build_test_app(
    allowed: Vec<String>,
    sim_checks: bool,
    max_drain: Option<u64>,
    simulator: ConfigurableSimulator,
) -> (axum::Router, TempDir) {
    let tmp = tempfile::tempdir().expect("temp dir");
    let db = tmp.path().join("audit.sled");
    let logger = Arc::new(AuditLogger::new(db.to_str().unwrap()).expect("logger"));
    let sim: Arc<dyn Simulate + Send + Sync> = Arc::new(simulator);
    let policy = Policy {
        max_sol_per_tx: None,
        max_balance_drain_lamports: max_drain,
        rate_limit_per_minute: None,
        allowed_programs: allowed,
        blocked_addresses: vec![],
        simulation_checks_enabled: sim_checks,
    };
    (build_app(policy, sim, logger), tmp)
}

fn simulate_request(tx_b64: &str, intent: Option<&str>) -> Request<Body> {
    let mut payload = json!({ "transaction": tx_b64 });
    if let Some(i) = intent {
        payload["intent"] = json!(i);
    }
    Request::builder()
        .method("POST")
        .uri("/simulate")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("request")
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

// ── Battery: Full-stack transaction type acceptance ───────────────────────

#[tokio::test]
async fn battery_jupiter_swap_allowed_and_logged() {
    let jup = jupiter_v6();
    let (app, _tmp) = build_test_app(
        vec![jup.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(50_000),
    );
    let tx = make_dex_swap_tx(jup);
    let b64 = encode_tx(&tx);

    let resp = app
        .clone()
        .oneshot(simulate_request(&b64, Some("jupiter swap")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify audit log
    let logs_resp = app.oneshot(get_request("/logs")).await.unwrap();
    let body = to_bytes(logs_resp.into_body(), usize::MAX).await.unwrap();
    let logs: Vec<AuditEntry> = serde_json::from_slice(&body).unwrap();
    assert!(logs.iter().any(|e| {
        matches!(e.decision, Decision::Allowed)
            && e.intent.as_deref() == Some("jupiter swap")
            && e.result == AuditResult::Allowed
    }));
}

#[tokio::test]
async fn battery_raydium_swap_allowed_and_logged() {
    let ray = raydium_amm();
    let (app, _tmp) = build_test_app(
        vec![ray.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(80_000),
    );
    let tx = make_dex_swap_tx(ray);

    let resp = app
        .clone()
        .oneshot(simulate_request(&encode_tx(&tx), Some("raydium swap")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let result: SimulationResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.units_consumed, Some(80_000));
}

#[tokio::test]
async fn battery_system_transfer_allowed() {
    let sys = system_program::id();
    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(5_000),
    );
    let tx = make_transfer_tx(1_000_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), Some("SOL transfer")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn battery_stake_delegate_allowed() {
    let stake_id = stake::program::id();
    let sys = system_program::id();
    let (app, _tmp) = build_test_app(
        vec![stake_id.to_string(), sys.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(30_000),
    );
    let tx = make_stake_delegate_tx();

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), Some("stake delegate")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn battery_stake_deactivate_allowed() {
    let stake_id = stake::program::id();
    let (app, _tmp) = build_test_app(
        vec![stake_id.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(20_000),
    );
    let tx = make_stake_deactivate_tx();

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), Some("stake deactivate")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Battery: Blocked transaction types ───────────────────────────────────

#[tokio::test]
async fn battery_jupiter_blocked_when_not_whitelisted() {
    let (app, _tmp) = build_test_app(
        vec![system_program::id().to_string()],
        true,
        None,
        ConfigurableSimulator::ok(50_000),
    );
    let tx = make_dex_swap_tx(jupiter_v6());

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(payload["error"]
        .as_str()
        .unwrap()
        .contains(&jupiter_v6().to_string()));
}

#[tokio::test]
async fn battery_raydium_blocked_when_not_whitelisted() {
    let (app, _tmp) = build_test_app(
        vec![jupiter_v6().to_string()],
        true,
        None,
        ConfigurableSimulator::ok(50_000),
    );
    let tx = make_dex_swap_tx(raydium_amm());

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn battery_transfer_blocked_when_not_whitelisted() {
    let (app, _tmp) = build_test_app(
        vec![jupiter_v6().to_string()],
        true,
        None,
        ConfigurableSimulator::ok(5_000),
    );
    let tx = make_transfer_tx(500_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ── Multi-instruction transactions ───────────────────────────────────────

#[tokio::test]
async fn multi_ix_allowed_when_all_programs_whitelisted() {
    let jup = jupiter_v6();
    let sys = system_program::id();
    let (app, _tmp) = build_test_app(
        vec![jup.to_string(), sys.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(120_000),
    );
    let tx = make_multi_ix_tx(&[sys, jup]);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn multi_ix_blocked_if_any_program_not_whitelisted() {
    let jup = jupiter_v6();
    let sys = system_program::id();
    let tok = token_program();
    let (app, _tmp) = build_test_app(
        vec![jup.to_string(), sys.to_string()], // token_program NOT whitelisted
        true,
        None,
        ConfigurableSimulator::ok(80_000),
    );
    let tx = make_multi_ix_tx(&[sys, jup, tok]);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ── Simulation-check enforcement through API ─────────────────────────────

#[tokio::test]
async fn simulation_error_produces_pending_approval_with_block_id() {
    let sys = system_program::id();
    let mut sim = ConfigurableSimulator::ok(80_000);
    sim.error = Some(json!({"InstructionError": [0, {"Custom": 6001}]}));

    let (app, _tmp) = build_test_app(vec![sys.to_string()], true, None, sim);
    let tx = make_transfer_tx(1_000_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), Some("error tx")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(payload["block_id"].as_str().is_some());
    assert!(payload["error"]
        .as_str()
        .unwrap()
        .contains("Simulation error"));
}

#[tokio::test]
async fn high_compute_units_produces_pending_approval() {
    let sys = system_program::id();
    let sim = ConfigurableSimulator::ok(999_999); // way over 200k limit

    let (app, _tmp) = build_test_app(vec![sys.to_string()], true, None, sim);
    let tx = make_transfer_tx(1_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(payload["block_id"].as_str().is_some());
}

#[tokio::test]
async fn drain_over_limit_produces_pending_approval() {
    let sys = system_program::id();
    let sim = ConfigurableSimulator::ok(50_000).with_drain("Wallet1", 5_000_000);

    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        true,
        Some(1_000_000), // 1M limit
        sim,
    );
    let tx = make_transfer_tx(5_000_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(payload["error"]
        .as_str()
        .unwrap()
        .contains("balance drain"));
}

#[tokio::test]
async fn drain_under_limit_passes_simulation() {
    let sys = system_program::id();
    let sim = ConfigurableSimulator::ok(50_000).with_drain("Wallet1", 500_000);

    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        true,
        Some(1_000_000), // 1M limit, drain is 500k
        sim,
    );
    let tx = make_transfer_tx(500_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ── Simulation checks disabled ──────────────────────────────────────────

#[tokio::test]
async fn sim_checks_disabled_allows_error_tx_through() {
    let sys = system_program::id();
    let mut sim = ConfigurableSimulator::ok(999_999);
    sim.error = Some(json!("ProgramFailedToComplete"));

    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        false, // sim checks OFF
        Some(100),
        sim,
    );
    let tx = make_transfer_tx(1_000);

    let resp = app
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let result: SimulationResult = serde_json::from_slice(&body).unwrap();
    assert!(result.error.is_some());
    assert_eq!(result.units_consumed, Some(999_999));
}

// ── Override workflow with different tx types ─────────────────────────────

#[tokio::test]
async fn override_allow_for_blocked_dex_swap() {
    let jup = jupiter_v6();
    let mut sim = ConfigurableSimulator::ok(50_000);
    sim.error = Some(json!("SlippageExceeded"));

    let (app, _tmp) = build_test_app(vec![jup.to_string()], true, None, sim);
    let tx = make_dex_swap_tx(jup);

    // Block it
    let resp = app
        .clone()
        .oneshot(simulate_request(&encode_tx(&tx), Some("risky swap")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let block_id = payload["block_id"].as_str().unwrap().to_string();

    // Override with ALLOW
    let override_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/override")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"block_id": block_id, "action": "ALLOW"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(override_resp.status(), StatusCode::OK);

    // Pending should be empty
    let pending = app.oneshot(get_request("/pending")).await.unwrap();
    let body = to_bytes(pending.into_body(), usize::MAX).await.unwrap();
    let p: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(p, json!({}));
}

#[tokio::test]
async fn override_reject_for_blocked_transfer() {
    let sys = system_program::id();
    let mut sim = ConfigurableSimulator::ok(50_000);
    sim.error = Some(json!("InsufficientFunds"));

    let (app, _tmp) = build_test_app(vec![sys.to_string()], true, None, sim);
    let tx = make_transfer_tx(99_000_000_000);

    let resp = app
        .clone()
        .oneshot(simulate_request(
            &encode_tx(&tx),
            Some("huge transfer"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let block_id = payload["block_id"].as_str().unwrap().to_string();

    let override_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/override")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"block_id": block_id, "action": "REJECT"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(override_resp.status(), StatusCode::FORBIDDEN);

    // Verify audit log records the rejection
    let logs_resp = app.oneshot(get_request("/logs")).await.unwrap();
    let body = to_bytes(logs_resp.into_body(), usize::MAX).await.unwrap();
    let logs: Vec<AuditEntry> = serde_json::from_slice(&body).unwrap();
    assert!(logs.iter().any(|e| {
        matches!(e.decision, Decision::Blocked(ref msg) if msg.contains("Rejected by human override"))
    }));
}

// ── Audit log content integrity ──────────────────────────────────────────

#[tokio::test]
async fn audit_logs_contain_transaction_details_for_allowed_tx() {
    let sys = system_program::id();
    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(10_000),
    );
    let tx = make_transfer_tx(42_000);

    let resp = app
        .clone()
        .oneshot(simulate_request(
            &encode_tx(&tx),
            Some("detail check"),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let logs_resp = app.oneshot(get_request("/logs")).await.unwrap();
    let body = to_bytes(logs_resp.into_body(), usize::MAX).await.unwrap();
    let logs: Vec<AuditEntry> = serde_json::from_slice(&body).unwrap();

    let entry = logs
        .iter()
        .find(|e| matches!(e.decision, Decision::Allowed))
        .expect("allowed entry");
    assert!(entry.transaction_details.is_some());
    let details = entry.transaction_details.as_ref().unwrap();
    assert!(details.request_payload_base64.is_some());
    assert!(!details.program_ids.is_empty());
    assert!(!details.account_keys.is_empty());
    assert!(details
        .program_ids
        .contains(&system_program::id().to_string()));
}

#[tokio::test]
async fn audit_logs_contain_simulation_result_for_allowed_tx() {
    let sys = system_program::id();
    let (app, _tmp) = build_test_app(
        vec![sys.to_string()],
        true,
        None,
        ConfigurableSimulator::ok(77_000),
    );
    let tx = make_transfer_tx(1_000);

    app.clone()
        .oneshot(simulate_request(&encode_tx(&tx), None))
        .await
        .unwrap();

    let logs_resp = app.oneshot(get_request("/logs")).await.unwrap();
    let body = to_bytes(logs_resp.into_body(), usize::MAX).await.unwrap();
    let logs: Vec<AuditEntry> = serde_json::from_slice(&body).unwrap();

    let entry = logs
        .iter()
        .find(|e| matches!(e.decision, Decision::Allowed))
        .expect("allowed entry");
    assert_eq!(
        entry
            .simulation_result
            .as_ref()
            .and_then(|r| r.units_consumed),
        Some(77_000)
    );
}

// ── Empty allowlist means any program is accepted ────────────────────────

#[tokio::test]
async fn empty_allowlist_allows_any_program() {
    let (app, _tmp) = build_test_app(
        vec![], // empty = allow all
        true,
        None,
        ConfigurableSimulator::ok(10_000),
    );

    // Jupiter
    let jup_resp = app
        .clone()
        .oneshot(simulate_request(
            &encode_tx(&make_dex_swap_tx(jupiter_v6())),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(jup_resp.status(), StatusCode::OK);

    // Raydium
    let ray_resp = app
        .clone()
        .oneshot(simulate_request(
            &encode_tx(&make_dex_swap_tx(raydium_amm())),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(ray_resp.status(), StatusCode::OK);

    // Transfer
    let xfer_resp = app
        .clone()
        .oneshot(simulate_request(
            &encode_tx(&make_transfer_tx(1_000)),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(xfer_resp.status(), StatusCode::OK);

    // Stake
    let stake_resp = app
        .oneshot(simulate_request(
            &encode_tx(&make_stake_delegate_tx()),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(stake_resp.status(), StatusCode::OK);
}

// ── Policy update affects subsequent simulation ──────────────────────────

#[tokio::test]
async fn dynamic_policy_update_unblocks_dex_swap() {
    let sys = system_program::id();
    let jup = jupiter_v6();
    let (app, _tmp) = build_test_app(
        vec![sys.to_string()], // only system allowed
        true,
        None,
        ConfigurableSimulator::ok(50_000),
    );
    let jup_tx = encode_tx(&make_dex_swap_tx(jup));

    // Initially blocked
    let resp = app
        .clone()
        .oneshot(simulate_request(&jup_tx, None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Update policy to add Jupiter
    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/policy")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"allowed_programs": [sys.to_string(), jup.to_string()]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);

    // Now allowed
    let resp2 = app
        .oneshot(simulate_request(&jup_tx, None))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
}
