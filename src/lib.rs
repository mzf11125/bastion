use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use base64::Engine as _;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use uuid::Uuid;

pub mod audit;
pub mod logger;
pub mod policy;
pub mod simulation;

use audit::{
    AuditEntry, AuditLogger, AuditResult, Decision, TransactionDetails,
    current_timestamp, hash_transaction_payload,
};
use policy::{MaxUnitsCheck, NoErrorCheck, Policy, PolicyEngine, SimulationCheck};
use simulation::{Simulate, SimulationResult};

#[derive(Clone, serde::Serialize)]
struct PendingApproval {
    #[serde(serialize_with = "serialize_tx")]
    transaction: solana_sdk::transaction::Transaction,
    simulation_result: SimulationResult,
    intent: Option<String>,
}

fn serialize_tx<S>(
    tx: &solana_sdk::transaction::Transaction,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bytes = bincode::serialize(tx).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[derive(Clone)]
struct AppState {
    policy_engine: Arc<RwLock<PolicyEngine>>,
    simulator: Arc<dyn Simulate + Send + Sync>,
    logger: Arc<AuditLogger>,
    pending_approvals: Arc<RwLock<HashMap<String, PendingApproval>>>,
    started_at: std::time::Instant,
}

#[derive(serde::Deserialize)]
struct SimulateRequest {
    transaction: String,
    intent: Option<String>,
}

#[derive(serde::Deserialize)]
struct UpdatePolicyRequest {
    allowed_programs: Vec<String>,
}

#[derive(serde::Deserialize)]
struct FullPolicyUpdateRequest {
    #[serde(default)]
    max_sol_per_tx: Option<u64>,
    #[serde(default)]
    max_balance_drain_lamports: Option<u64>,
    #[serde(default)]
    rate_limit_per_minute: Option<u32>,
    #[serde(default)]
    allowed_programs: Option<Vec<String>>,
    #[serde(default)]
    blocked_addresses: Option<Vec<String>>,
    #[serde(default)]
    simulation_checks_enabled: Option<bool>,
}

#[derive(serde::Serialize)]
struct PolicyResponse {
    allowed_programs: Vec<String>,
}

#[derive(serde::Serialize)]
struct FullPolicyResponse {
    max_sol_per_tx: Option<u64>,
    max_balance_drain_lamports: Option<u64>,
    rate_limit_per_minute: Option<u32>,
    allowed_programs: Vec<String>,
    blocked_addresses: Vec<String>,
    simulation_checks_enabled: bool,
}

#[derive(serde::Serialize)]
struct PaginatedLogsResponse {
    total: usize,
    offset: usize,
    limit: usize,
    entries: Vec<AuditEntry>,
}

#[derive(serde::Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_seconds: u64,
    db_healthy: bool,
    db_size_bytes: u64,
}

#[derive(serde::Serialize)]
struct ClearResponse {
    cleared: u64,
}

#[derive(serde::Serialize)]
struct DeleteResponse {
    deleted: bool,
}

#[derive(serde::Deserialize)]
struct LogsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    transaction_id: Option<String>,
    signature: Option<String>,
    result: Option<AuditResult>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OverrideAction {
    Allow,
    Reject,
}

#[derive(serde::Deserialize)]
struct OverrideRequest {
    block_id: String,
    action: OverrideAction,
}

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_id: Option<String>,
}

async fn hello() -> &'static str {
    "Hello, world!"
}

async fn update_policy(
    State(state): State<AppState>,
    Json(request): Json<UpdatePolicyRequest>,
) -> Json<PolicyResponse> {
    let mut policy_engine = state.policy_engine.write().await;
    policy_engine.update_allowed_programs(request.allowed_programs);
    Json(PolicyResponse {
        allowed_programs: policy_engine.allowed_programs(),
    })
}

async fn get_policy(State(state): State<AppState>) -> Json<FullPolicyResponse> {
    let policy_engine = state.policy_engine.read().await;
    let snapshot = policy_engine.policy_snapshot();
    Json(FullPolicyResponse {
        max_sol_per_tx: snapshot.max_sol_per_tx,
        max_balance_drain_lamports: snapshot.max_balance_drain_lamports,
        rate_limit_per_minute: snapshot.rate_limit_per_minute,
        allowed_programs: snapshot.allowed_programs,
        blocked_addresses: snapshot.blocked_addresses,
        simulation_checks_enabled: snapshot.simulation_checks_enabled,
    })
}

fn build_audit_entry(
    transaction_signature: Option<String>,
    decision: Decision,
    result: AuditResult,
    reasoning: String,
    simulation_result: Option<SimulationResult>,
    intent: Option<String>,
    transaction_details: Option<TransactionDetails>,
) -> AuditEntry {
    let simulation_logs = simulation_result
        .as_ref()
        .map(|result| result.logs.clone())
        .unwrap_or_default();
    let transaction_id = transaction_signature.clone().or_else(|| {
        transaction_details
            .as_ref()
            .and_then(|details| details.request_payload_base64.as_ref())
            .map(|payload| hash_transaction_payload(payload))
    });

    AuditEntry {
        timestamp: current_timestamp(),
        transaction_id,
        transaction_signature,
        decision,
        simulation_result,
        intent,
        result,
        reasoning,
        simulation_logs,
        transaction_details,
    }
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        uptime_seconds: state.started_at.elapsed().as_secs(),
        db_healthy: state.logger.is_healthy(),
        db_size_bytes: state.logger.size_on_disk(),
    })
}

async fn update_full_policy(
    State(state): State<AppState>,
    Json(request): Json<FullPolicyUpdateRequest>,
) -> Json<FullPolicyResponse> {
    let mut policy_engine = state.policy_engine.write().await;
    policy_engine.update_policy(
        request.max_sol_per_tx,
        request.max_balance_drain_lamports,
        request.rate_limit_per_minute,
        request.allowed_programs,
        request.blocked_addresses,
        request.simulation_checks_enabled,
    );
    let snapshot = policy_engine.policy_snapshot();
    Json(FullPolicyResponse {
        max_sol_per_tx: snapshot.max_sol_per_tx,
        max_balance_drain_lamports: snapshot.max_balance_drain_lamports,
        rate_limit_per_minute: snapshot.rate_limit_per_minute,
        allowed_programs: snapshot.allowed_programs,
        blocked_addresses: snapshot.blocked_addresses,
        simulation_checks_enabled: snapshot.simulation_checks_enabled,
    })
}

async fn export_policy_toml(State(state): State<AppState>) -> impl IntoResponse {
    let policy_engine = state.policy_engine.read().await;
    let snapshot = policy_engine.policy_snapshot();
    match toml::to_string_pretty(&snapshot) {
        Ok(toml_str) => (
            StatusCode::OK,
            [("content-type", "application/toml")],
            toml_str,
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to serialize policy: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn get_audit_stats(State(state): State<AppState>) -> impl IntoResponse {
    match state.logger.count(None) {
        Ok(stats) => Json(stats).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to compute audit stats: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn clear_audit_logs(State(state): State<AppState>) -> impl IntoResponse {
    match state.logger.clear() {
        Ok(cleared) => Json(ClearResponse { cleared }).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to clear audit logs: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn delete_audit_log(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.logger.delete_by_id(id) {
        Ok(true) => Json(DeleteResponse { deleted: true }).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Audit log entry {id} not found"),
                block_id: None,
            }),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to delete audit log: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn simulate(
    State(state): State<AppState>,
    Json(request): Json<SimulateRequest>,
) -> impl IntoResponse {
    let intent = request.intent.clone();
    let request_payload = request.transaction.clone();
    let request_details = TransactionDetails::from_request_payload(request_payload.clone());

    let tx_bytes = match base64::engine::general_purpose::STANDARD.decode(&request.transaction) {
        Ok(bytes) => bytes,
        Err(err) => {
            let reason = format!("Invalid base64 transaction: {err}");
            let entry = AuditEntry {
                transaction_signature: None,
                ..build_audit_entry(
                    None,
                    Decision::Blocked(reason.clone()),
                    AuditResult::Blocked,
                    reason.clone(),
                    None,
                    intent.clone(),
                    Some(request_details.clone()),
                )
            };
            let _ = state.logger.log(entry);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: reason,
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    let tx: solana_sdk::transaction::Transaction = match bincode::deserialize(&tx_bytes) {
        Ok(tx) => tx,
        Err(err) => {
            let reason = format!("Invalid transaction payload: {err}");
            let entry = AuditEntry {
                transaction_signature: None,
                ..build_audit_entry(
                    None,
                    Decision::Blocked(reason.clone()),
                    AuditResult::Blocked,
                    reason.clone(),
                    None,
                    intent.clone(),
                    Some(request_details.clone()),
                )
            };
            let _ = state.logger.log(entry);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: reason,
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    let tx_details = TransactionDetails::from_transaction_request(request_payload, &tx);
    let signature = tx_details.signature.clone();

    let policy_check = {
        let engine = state.policy_engine.read().await;
        engine.check_transaction(&tx)
    };

    if let Err(err) = policy_check {
        let entry = AuditEntry {
            ..build_audit_entry(
                signature.clone(),
                Decision::Blocked(err.clone()),
                AuditResult::Blocked,
                err.clone(),
                None,
                intent.clone(),
                Some(tx_details.clone()),
            )
        };
        let _ = state.logger.log(entry);

        return (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: err,
                block_id: None,
            }),
        )
            .into_response();
    }

    let simulator = state.simulator.clone();
    let tx_clone = tx.clone();
    let spawn_result =
        tokio::task::spawn_blocking(move || simulator.simulate_transaction(&tx_clone)).await;

    let res = match spawn_result {
        Ok(r) => r,
        Err(err) => {
            let reason = format!("Simulation task failed: {err}");
            let entry = AuditEntry {
                ..build_audit_entry(
                    signature.clone(),
                    Decision::Blocked(reason.clone()),
                    AuditResult::Blocked,
                    reason.clone(),
                    None,
                    intent.clone(),
                    Some(tx_details.clone()),
                )
            };
            let _ = state.logger.log(entry);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: reason,
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    let result = match res {
        Ok(r) => r,
        Err(err) => {
            let reason = format!("Simulation failed: {err}");
            let entry = AuditEntry {
                ..build_audit_entry(
                    signature.clone(),
                    Decision::Blocked(reason.clone()),
                    AuditResult::Blocked,
                    reason.clone(),
                    None,
                    intent.clone(),
                    Some(tx_details.clone()),
                )
            };
            let _ = state.logger.log(entry);
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: reason,
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    let simulation_checks_enabled = {
        let engine = state.policy_engine.read().await;
        engine.simulation_checks_enabled()
    };

    if simulation_checks_enabled {
        let max_balance_drain = {
            let engine = state.policy_engine.read().await;
            engine.max_balance_drain_lamports()
        };

        let checks: Vec<Box<dyn SimulationCheck>> = if let Some(limit) = max_balance_drain {
            vec![
                Box::new(NoErrorCheck),
                Box::new(MaxUnitsCheck),
                Box::new(policy::MaxBalanceDrainCheck { limit }),
            ]
        } else {
            vec![Box::new(NoErrorCheck), Box::new(MaxUnitsCheck)]
        };

        for check in checks {
            if let Err(err) = check.check(&result) {
                let block_id = Uuid::new_v4().to_string();

                let entry = AuditEntry {
                    ..build_audit_entry(
                        signature.clone(),
                        Decision::PendingApproval(block_id.clone()),
                        AuditResult::Blocked,
                        err.clone(),
                        Some(result.clone()),
                        intent.clone(),
                        Some(tx_details.clone()),
                    )
                };
                let _ = state.logger.log(entry);

                let mut pending_approvals = state.pending_approvals.write().await;
                pending_approvals.insert(
                    block_id.clone(),
                    PendingApproval {
                        transaction: tx,
                        simulation_result: result.clone(),
                        intent,
                    },
                );

                return (
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse {
                        error: err,
                        block_id: Some(block_id),
                    }),
                )
                    .into_response();
            }
        }
    }

    let entry = AuditEntry {
        ..build_audit_entry(
            signature,
            Decision::Allowed,
            AuditResult::Allowed,
            "All policy and simulation checks passed".to_string(),
            Some(result.clone()),
            intent.clone(),
            Some(tx_details),
        )
    };
    let _ = state.logger.log(entry);

    Json(result).into_response()
}

async fn get_logs(
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    let LogsQuery {
        limit,
        offset,
        transaction_id,
        signature,
        result,
    } = query;

    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let total = match state.logger.count_filtered(
        transaction_id.as_deref(),
        signature.as_deref(),
        result,
    ) {
        Ok(t) => t,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to count logs: {err}"),
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    match state.logger.get_logs_filtered(
        transaction_id.as_deref(),
        signature.as_deref(),
        result,
        offset,
        limit,
    ) {
        Ok(entries) => Json(PaginatedLogsResponse {
            total,
            offset,
            limit,
            entries,
        })
        .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to retrieve logs: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn get_logs_by_transaction_id(
    State(state): State<AppState>,
    Path(transaction_id): Path<String>,
) -> impl IntoResponse {
    match state.logger.get_logs_by_transaction_id(&transaction_id) {
        Ok(logs) => Json(logs).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to retrieve logs: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn get_logs_by_signature(
    State(state): State<AppState>,
    Path(signature): Path<String>,
) -> impl IntoResponse {
    match state.logger.get_logs_by_signature(&signature) {
        Ok(logs) => Json(logs).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to retrieve logs: {err}"),
                block_id: None,
            }),
        )
            .into_response(),
    }
}

async fn get_pending(State(state): State<AppState>) -> impl IntoResponse {
    let pending = state.pending_approvals.read().await;
    Json(pending.clone()).into_response()
}

async fn override_block(
    State(state): State<AppState>,
    Json(request): Json<OverrideRequest>,
) -> impl IntoResponse {
    let mut pending_approvals = state.pending_approvals.write().await;
    let pending = match pending_approvals.remove(&request.block_id) {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Block ID not found".to_string(),
                    block_id: None,
                }),
            )
                .into_response();
        }
    };

    let tx_details = TransactionDetails::from_transaction(&pending.transaction);
    let signature = tx_details.signature.clone();

    match request.action {
        OverrideAction::Allow => {
            let reason = format!(
                "Approved by human override for block_id={}",
                request.block_id
            );
            let entry = AuditEntry {
                ..build_audit_entry(
                    signature,
                    Decision::Allowed,
                    AuditResult::Allowed,
                    reason,
                    Some(pending.simulation_result.clone()),
                    pending.intent,
                    Some(tx_details),
                )
            };
            let _ = state.logger.log(entry);
            Json(pending.simulation_result).into_response()
        }
        OverrideAction::Reject => {
            let reason = "Rejected by human override".to_string();
            let entry = AuditEntry {
                ..build_audit_entry(
                    signature,
                    Decision::Blocked(reason.clone()),
                    AuditResult::Blocked,
                    reason.clone(),
                    Some(pending.simulation_result),
                    pending.intent,
                    Some(tx_details),
                )
            };
            let _ = state.logger.log(entry);
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: reason,
                    block_id: None,
                }),
            )
                .into_response()
        }
    }
}

pub fn build_app(
    policy: Policy,
    simulator: Arc<dyn Simulate + Send + Sync>,
    logger: Arc<AuditLogger>,
) -> Router {
    let app_state = AppState {
        policy_engine: Arc::new(RwLock::new(PolicyEngine::new(policy))),
        simulator,
        logger,
        pending_approvals: Arc::new(RwLock::new(HashMap::new())),
        started_at: std::time::Instant::now(),
    };

    Router::new()
        .route("/", get(hello))
        .route("/health", get(health))
        .route("/simulate", post(simulate))
        // Audit log endpoints
        .route("/logs", get(get_logs))
        .route("/logs/tx/:transaction_id", get(get_logs_by_transaction_id))
        .route("/logs/signature/:signature", get(get_logs_by_signature))
        .route("/audit/logs", get(get_logs))
        .route(
            "/audit/logs/tx/:transaction_id",
            get(get_logs_by_transaction_id),
        )
        .route(
            "/audit/logs/signature/:signature",
            get(get_logs_by_signature),
        )
        .route("/audit/stats", get(get_audit_stats))
        .route("/audit/logs/clear", post(clear_audit_logs))
        .route("/audit/logs/:id", axum::routing::delete(delete_audit_log))
        // Pending approvals & override
        .route("/pending", get(get_pending))
        .route("/override", post(override_block))
        // Policy endpoints
        .route(
            "/policy",
            get(get_policy).post(update_policy).put(update_policy),
        )
        .route(
            "/policy/allowed-programs",
            post(update_policy).put(update_policy),
        )
        .route("/policy/full", post(update_full_policy).put(update_full_policy))
        .route("/policy/export", get(export_policy_toml))
        // Static dashboard
        .nest_service("/dashboard", ServeDir::new("static"))
        .with_state(app_state)
}
