use crate::decision::FirewallDecision;
use crate::policy::set::PolicySet;
use crate::policy::types::PolicyRule;
use crate::risk::RiskOracle;
use crate::transaction::{Address, NormalizedTransaction, TxType};

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Sliding-window rate limiter state.
#[derive(Default)]
struct RateLimitState {
    /// Timestamps of recent transactions for frequency tracking.
    recent_txs: Vec<Instant>,
    /// Cumulative 24h volume per currency.
    volume_24h: HashMap<String, u64>,
}

/// Core policy evaluator — evaluates a normalized transaction against a policy set.
pub struct PolicyEvaluator<O: RiskOracle> {
    rate_state: Mutex<RateLimitState>,
    oracle: Option<O>,
}

impl<O: RiskOracle> PolicyEvaluator<O> {
    /// Create a new evaluator without a risk oracle.
    pub fn new() -> Self {
        Self {
            rate_state: Mutex::new(RateLimitState::default()),
            oracle: None,
        }
    }

    /// Create a new evaluator with a risk oracle.
    pub fn with_oracle(oracle: O) -> Self {
        Self {
            rate_state: Mutex::new(RateLimitState::default()),
            oracle: Some(oracle),
        }
    }

    /// Evaluate a normalized transaction against the given policy set.
    ///
    /// Returns `Pass` if all rules permit it, or a block/HITL decision
    /// on the first violation encountered.
    pub async fn evaluate(
        &self,
        tx: &NormalizedTransaction,
        policy: &PolicySet,
    ) -> FirewallDecision {
        if policy.is_empty() {
            return FirewallDecision::Pass;
        }

        for rule in &policy.rules {
            let result = self.evaluate_rule(tx, rule).await;
            if !matches!(result, FirewallDecision::Pass) {
                return result;
            }
        }

        FirewallDecision::Pass
    }

    async fn evaluate_rule(
        &self,
        tx: &NormalizedTransaction,
        rule: &PolicyRule,
    ) -> FirewallDecision {
        match rule {
            PolicyRule::AmountLimit {
                max_per_transaction,
                max_per_24h,
                currency,
            } => self.check_amount(tx, *max_per_transaction, *max_per_24h, currency),

            PolicyRule::Destination {
                allowlist,
                blocklist,
            } => self.check_destination(tx, allowlist, blocklist),

            PolicyRule::Frequency {
                max_transactions_per_hour,
            } => self.check_frequency(tx, *max_transactions_per_hour),

            PolicyRule::HITL {
                trigger_above,
                timeout_seconds,
            } => self.check_hitl(tx, *trigger_above, *timeout_seconds),

            PolicyRule::Reputation {
                minimum_score,
                elevated_limit_multiplier: _,
            } => self.check_reputation(tx, *minimum_score).await,

            PolicyRule::TxTypeAllowlist { allowed } => self.check_tx_type(tx, allowed),
        }
    }

    fn check_amount(
        &self,
        tx: &NormalizedTransaction,
        max_per_tx: u64,
        max_per_24h: Option<u64>,
        currency: &str,
    ) -> FirewallDecision {
        if tx.amount > max_per_tx {
            return FirewallDecision::Block {
                reason: format!(
                    "Transaction amount {} {} exceeds max_per_transaction limit {} {}",
                    tx.amount, tx.currency, max_per_tx, currency
                ),
                policy_id: None,
            };
        }

        if let Some(limit_24h) = max_per_24h {
            let mut state = self.rate_state.lock().unwrap();
            let total = state.volume_24h.entry(currency.to_string()).or_insert(0);
            *total += tx.amount;

            if *total > limit_24h {
                return FirewallDecision::Block {
                    reason: format!(
                        "24h volume {} {} exceeds limit {} {}",
                        total, tx.currency, limit_24h, currency
                    ),
                    policy_id: None,
                };
            }
        }

        FirewallDecision::Pass
    }

    fn check_destination(
        &self,
        tx: &NormalizedTransaction,
        allowlist: &[Address],
        blocklist: &[Address],
    ) -> FirewallDecision {
        // Blocklist takes precedence
        if blocklist.iter().any(|addr| addr == &tx.to) {
            return FirewallDecision::Block {
                reason: format!("Destination {} is on the blocklist", tx.to),
                policy_id: None,
            };
        }

        // If allowlist is non-empty, destination must be in it
        if !allowlist.is_empty() && !allowlist.iter().any(|addr| addr == &tx.to) {
            return FirewallDecision::Block {
                reason: format!("Destination {} is not on the allowlist", tx.to),
                policy_id: None,
            };
        }

        FirewallDecision::Pass
    }

    fn check_frequency(&self, _tx: &NormalizedTransaction, max_per_hour: u32) -> FirewallDecision {
        let mut state = self.rate_state.lock().unwrap();
        let now = Instant::now();
        let window = Duration::from_secs(3600);

        // Prune old entries
        state.recent_txs.retain(|t| now.duration_since(*t) < window);
        state.recent_txs.push(now);

        if state.recent_txs.len() > max_per_hour as usize {
            return FirewallDecision::Block {
                reason: format!(
                    "Rate limit exceeded: {} transactions in the last hour (max {})",
                    state.recent_txs.len(),
                    max_per_hour
                ),
                policy_id: None,
            };
        }

        FirewallDecision::Pass
    }

    fn check_hitl(
        &self,
        tx: &NormalizedTransaction,
        trigger_above: u64,
        _timeout_seconds: u64,
    ) -> FirewallDecision {
        if tx.amount > trigger_above {
            let approval_id = uuid::Uuid::new_v4().to_string();
            return FirewallDecision::PendingHITL {
                approval_id,
                reason: format!(
                    "Transaction amount {} exceeds HITL threshold {}",
                    tx.amount, trigger_above
                ),
            };
        }
        FirewallDecision::Pass
    }

    async fn check_reputation(
        &self,
        tx: &NormalizedTransaction,
        minimum_score: u8,
    ) -> FirewallDecision {
        if let Some(ref oracle) = self.oracle {
            match oracle.score(&tx.to).await {
                Ok(score) if score.value() >= minimum_score => FirewallDecision::Pass,
                Ok(score) => FirewallDecision::Block {
                    reason: format!(
                        "Reputation score {} is below minimum {} for destination {}",
                        score.value(),
                        minimum_score,
                        tx.to
                    ),
                    policy_id: None,
                },
                Err(e) => FirewallDecision::Block {
                    reason: format!("Risk oracle error: {}", e),
                    policy_id: None,
                },
            }
        } else {
            // No oracle configured — allow through
            FirewallDecision::Pass
        }
    }

    fn check_tx_type(&self, tx: &NormalizedTransaction, allowed: &[String]) -> FirewallDecision {
        let type_str = match tx.tx_type {
            TxType::Transfer => "transfer",
            TxType::Payment => "payment",
            TxType::Governance => "governance",
            TxType::Custom => "custom",
        };

        if !allowed.iter().any(|a| a == type_str) {
            return FirewallDecision::Block {
                reason: format!(
                    "Transaction type '{}' is not allowed. Allowed types: {:?}",
                    type_str, allowed
                ),
                policy_id: None,
            };
        }
        FirewallDecision::Pass
    }
}

impl<O: RiskOracle> Default for PolicyEvaluator<O> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction::{Chain, NormalizedTransaction};

    /// A no-op oracle for testing.
    struct NoopOracle;
    #[async_trait::async_trait]
    impl RiskOracle for NoopOracle {
        async fn score(
            &self,
            _address: &Address,
        ) -> Result<crate::risk::RiskScore, crate::risk::RiskOracleError> {
            Ok(crate::risk::RiskScore::new(0))
        }
        fn provider_name(&self) -> &str {
            "noop"
        }
    }

    fn make_tx(amount: u64, to: &str, tx_type: TxType) -> NormalizedTransaction {
        NormalizedTransaction::new(
            "agent-1",
            "from-addr",
            to,
            amount,
            "SOL",
            tx_type,
            Chain::Solana,
        )
    }

    #[tokio::test]
    async fn test_empty_policy_passes() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "good-addr", TxType::Transfer);
        let policy = PolicySet::new();
        assert_eq!(
            evaluator.evaluate(&tx, &policy).await,
            FirewallDecision::Pass
        );
    }

    #[tokio::test]
    async fn test_amount_limit_blocks() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(10_001, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::AmountLimit {
            max_per_transaction: 10_000,
            max_per_24h: None,
            currency: "SOL".into(),
        });
        let result = evaluator.evaluate(&tx, &policy).await;
        assert!(result.is_blocked());
    }

    #[tokio::test]
    async fn test_amount_limit_passes_within_limit() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(5_000, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::AmountLimit {
            max_per_transaction: 10_000,
            max_per_24h: None,
            currency: "SOL".into(),
        });
        assert_eq!(
            evaluator.evaluate(&tx, &policy).await,
            FirewallDecision::Pass
        );
    }

    #[tokio::test]
    async fn test_destination_allowlist_blocks_unknown() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "bad-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::Destination {
            allowlist: vec![Address::new("good-addr")],
            blocklist: vec![],
        });
        let result = evaluator.evaluate(&tx, &policy).await;
        assert!(result.is_blocked());
    }

    #[tokio::test]
    async fn test_destination_allowlist_passes_known() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::Destination {
            allowlist: vec![Address::new("good-addr")],
            blocklist: vec![],
        });
        assert_eq!(
            evaluator.evaluate(&tx, &policy).await,
            FirewallDecision::Pass
        );
    }

    #[tokio::test]
    async fn test_destination_blocklist_blocks() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "evil-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::Destination {
            allowlist: vec![],
            blocklist: vec![Address::new("evil-addr")],
        });
        let result = evaluator.evaluate(&tx, &policy).await;
        assert!(result.is_blocked());
    }

    #[tokio::test]
    async fn test_hitl_triggers_above_threshold() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(60_000, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::HITL {
            trigger_above: 50_000,
            timeout_seconds: 3600,
        });
        let result = evaluator.evaluate(&tx, &policy).await;
        assert!(result.is_pending_hitl());
    }

    #[tokio::test]
    async fn test_hitl_passes_below_threshold() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(10_000, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::HITL {
            trigger_above: 50_000,
            timeout_seconds: 3600,
        });
        assert_eq!(
            evaluator.evaluate(&tx, &policy).await,
            FirewallDecision::Pass
        );
    }

    #[tokio::test]
    async fn test_tx_type_allowlist_blocks() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "good-addr", TxType::Transfer);
        let policy = PolicySet::new().with_rule(PolicyRule::TxTypeAllowlist {
            allowed: vec!["payment".into()],
        });
        let result = evaluator.evaluate(&tx, &policy).await;
        assert!(result.is_blocked());
    }

    #[tokio::test]
    async fn test_tx_type_allowlist_passes() {
        let evaluator = PolicyEvaluator::<NoopOracle>::new();
        let tx = make_tx(100, "good-addr", TxType::Payment);
        let policy = PolicySet::new().with_rule(PolicyRule::TxTypeAllowlist {
            allowed: vec!["payment".into()],
        });
        assert_eq!(
            evaluator.evaluate(&tx, &policy).await,
            FirewallDecision::Pass
        );
    }
}
