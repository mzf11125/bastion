//! Bridge between bastion-core PolicyEvaluator and the sidecar HTTP API.
//!
//! This module converts chain-specific transaction formats into
//! `NormalizedTransaction` and routes them through the chain-agnostic
//! policy engine.

use bastion_core::{
    Chain, FirewallDecision, NormalizedTransaction, PolicyEvaluator, PolicyRule, PolicySet, TxType,
};

use crate::grond_oracle::GrondOracle;

use serde::{Deserialize, Serialize};

/// JSON request body for the v2 evaluate endpoint.
#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub agent_id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub currency: Option<String>,
    pub tx_type: Option<String>,
    pub chain: Option<String>,
}

/// JSON response for the v2 evaluate endpoint.
#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_id: Option<String>,
}

/// Convert an EvaluateRequest into a NormalizedTransaction.
fn normalize_request(req: &EvaluateRequest) -> NormalizedTransaction {
    let chain = match req.chain.as_deref() {
        Some("base") | Some("ethereum") | Some("polygon") | Some("arbitrum") => Chain::Base,
        Some("midnight") => Chain::Midnight,
        _ => Chain::Solana,
    };

    let tx_type = match req.tx_type.as_deref() {
        Some("payment") => TxType::Payment,
        Some("governance") => TxType::Governance,
        Some("custom") => TxType::Custom,
        _ => TxType::Transfer,
    };

    NormalizedTransaction::new(
        &req.agent_id,
        &req.from,
        &req.to,
        req.amount,
        req.currency.as_deref().unwrap_or("SOL"),
        tx_type,
        chain,
    )
}

/// Default policy set for the sidecar.
/// In production, this would be loaded from on-chain state or config.
pub fn default_policy_set() -> PolicySet {
    PolicySet::new()
        .with_rule(PolicyRule::AmountLimit {
            max_per_transaction: 10_000_000_000, // 10 SOL
            max_per_24h: Some(100_000_000_000),  // 100 SOL per 24h
            currency: "SOL".into(),
        })
        .with_rule(PolicyRule::HITL {
            trigger_above: 50_000_000_000, // 50 SOL
            timeout_seconds: 3600,
        })
        .with_rule(PolicyRule::TxTypeAllowlist {
            allowed: vec!["transfer".into(), "payment".into()],
        })
}

/// Evaluate a transaction using the core policy engine.
///
/// When `grond` is `Some`, the evaluator uses GrondOSINT as its risk oracle.
pub async fn evaluate_core(
    req: EvaluateRequest,
    grond: Option<GrondOracle>,
) -> EvaluateResponse {
    evaluate_core_with_policy(req, default_policy_set(), grond).await
}

/// Evaluate with a custom policy set (used for testing).
pub async fn evaluate_core_with_policy(
    req: EvaluateRequest,
    policy: PolicySet,
    grond: Option<GrondOracle>,
) -> EvaluateResponse {
    let tx = normalize_request(&req);
    let evaluator = if let Some(oracle) = grond {
        PolicyEvaluator::with_oracle(oracle)
    } else {
        PolicyEvaluator::new()
    };

    let decision = evaluator.evaluate(&tx, &policy).await;

    match decision {
        FirewallDecision::Pass => EvaluateResponse {
            status: "passed".into(),
            reason: None,
            approval_id: None,
            policy_id: None,
        },
        FirewallDecision::Block { reason, policy_id } => EvaluateResponse {
            status: "blocked".into(),
            reason: Some(reason),
            approval_id: None,
            policy_id,
        },
        FirewallDecision::PendingHITL {
            approval_id,
            reason,
        } => EvaluateResponse {
            status: "pending_hitl".into(),
            reason: Some(reason),
            approval_id: Some(approval_id),
            policy_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_evaluate_core_pass() {
        let req = EvaluateRequest {
            agent_id: "agent-1".into(),
            from: "from-addr".into(),
            to: "to-addr".into(),
            amount: 1_000_000_000, // 1 SOL
            currency: Some("SOL".into()),
            tx_type: Some("transfer".into()),
            chain: Some("solana".into()),
        };
        let resp = evaluate_core(req, None).await;
        assert_eq!(resp.status, "passed");
    }

    #[tokio::test]
    async fn test_evaluate_core_block_amount() {
        let req = EvaluateRequest {
            agent_id: "agent-1".into(),
            from: "from-addr".into(),
            to: "to-addr".into(),
            amount: 20_000_000_000, // 20 SOL — exceeds 10 SOL limit
            currency: Some("SOL".into()),
            tx_type: Some("transfer".into()),
            chain: Some("solana".into()),
        };
        let resp = evaluate_core(req, None).await;
        assert_eq!(resp.status, "blocked");
    }

    #[tokio::test]
    async fn test_evaluate_core_hitl() {
        let policy = PolicySet::new()
            .with_rule(PolicyRule::AmountLimit {
                max_per_transaction: 100_000_000_000, // 100 SOL — high enough
                max_per_24h: None,
                currency: "SOL".into(),
            })
            .with_rule(PolicyRule::HITL {
                trigger_above: 50_000_000_000, // 50 SOL
                timeout_seconds: 3600,
            });
        let req = EvaluateRequest {
            agent_id: "agent-1".into(),
            from: "from-addr".into(),
            to: "to-addr".into(),
            amount: 60_000_000_000, // 60 SOL — exceeds 50 SOL HITL threshold
            currency: Some("SOL".into()),
            tx_type: Some("transfer".into()),
            chain: Some("solana".into()),
        };
        let resp = evaluate_core_with_policy(req, policy, None).await;
        assert_eq!(resp.status, "pending_hitl");
        assert!(resp.approval_id.is_some());
    }

    #[tokio::test]
    async fn test_evaluate_core_block_tx_type() {
        let req = EvaluateRequest {
            agent_id: "agent-1".into(),
            from: "from-addr".into(),
            to: "to-addr".into(),
            amount: 1_000_000_000,
            currency: Some("SOL".into()),
            tx_type: Some("governance".into()), // not in allowlist
            chain: Some("solana".into()),
        };
        let resp = evaluate_core(req, None).await;
        assert_eq!(resp.status, "blocked");
    }

    #[tokio::test]
    async fn test_normalize_request_evm_chain() {
        let req = EvaluateRequest {
            agent_id: "agent-2".into(),
            from: "0xabc".into(),
            to: "0xdef".into(),
            amount: 1_000_000,
            currency: Some("USDC".into()),
            tx_type: Some("payment".into()),
            chain: Some("base".into()),
        };
        let tx = normalize_request(&req);
        assert!(matches!(tx.chain, Chain::Base));
        assert_eq!(tx.currency, "USDC");
        assert!(matches!(tx.tx_type, TxType::Payment));
    }
}
