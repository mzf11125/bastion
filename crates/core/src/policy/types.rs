use crate::transaction::Address;
use serde::{Deserialize, Serialize};

/// A single policy rule that gates agent transactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyRule {
    /// Cap the maximum value per single transaction.
    AmountLimit {
        max_per_transaction: u64,
        max_per_24h: Option<u64>,
        currency: String,
    },
    /// Restrict which destination addresses an agent may interact with.
    Destination {
        allowlist: Vec<Address>,
        blocklist: Vec<Address>,
    },
    /// Limit transaction frequency.
    Frequency { max_transactions_per_hour: u32 },
    /// Require human approval for transactions above a threshold.
    HITL {
        trigger_above: u64,
        timeout_seconds: u64,
    },
    /// Require a minimum reputation score for high-value transactions.
    Reputation {
        minimum_score: u8,
        elevated_limit_multiplier: Option<f64>,
    },
    /// Restrict which transaction types are allowed.
    TxTypeAllowlist { allowed: Vec<String> },
}

impl PolicyRule {
    pub fn rule_name(&self) -> &'static str {
        match self {
            Self::AmountLimit { .. } => "amount_limit",
            Self::Destination { .. } => "destination",
            Self::Frequency { .. } => "frequency",
            Self::HITL { .. } => "hitl",
            Self::Reputation { .. } => "reputation",
            Self::TxTypeAllowlist { .. } => "tx_type_allowlist",
        }
    }
}
