use crate::simulation::SimulationResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use solana_sdk::transaction::Transaction;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Policy {
    #[serde(default)]
    pub max_sol_per_tx: Option<u64>,
    #[serde(default)]
    pub max_balance_drain_lamports: Option<u64>,
    #[serde(default)]
    pub rate_limit_per_minute: Option<u32>,
    #[serde(default)]
    pub allowed_programs: Vec<String>,
    #[serde(default)]
    pub blocked_addresses: Vec<String>,
    #[serde(default)]
    pub simulation_checks_enabled: bool,
}

impl Policy {
    pub fn check_transaction(&self, tx: &Transaction) -> Result<(), String> {
        for instruction in &tx.message.instructions {
            let program_id_index = usize::from(instruction.program_id_index);
            let program_id = tx
                .message
                .account_keys
                .get(program_id_index)
                .ok_or_else(|| format!("Invalid program_id_index: {}", program_id_index))?;
            let program_id_str = program_id.to_string();

            if !self.allowed_programs.is_empty()
                && !self
                    .allowed_programs
                    .iter()
                    .any(|allowed_program| allowed_program == &program_id_str)
            {
                return Err(format!("Program not allowed: {}", program_id));
            }
        }

        if let Some(limit) = self.max_sol_per_tx {
            let total_sol = Self::compute_total_sol_outflow(tx);
            if total_sol > limit {
                return Err(format!(
                    "Transaction value {} SOL exceeds max_sol_per_tx limit {}",
                    total_sol, limit
                ));
            }
        }

        if !self.blocked_addresses.is_empty() {
            Self::check_blocked_addresses(tx, &self.blocked_addresses)?;
        }

        Ok(())
    }

    fn compute_total_sol_outflow(tx: &Transaction) -> u64 {
        let mut total_lamports: u64 = 0;

        for instruction in &tx.message.instructions {
            let program_id_index = usize::from(instruction.program_id_index);
            let program_id = match tx.message.account_keys.get(program_id_index) {
                Some(p) => p,
                None => continue,
            };

            let program_str = program_id.to_string();

            if program_str == "System1111111111111111111111111111111" && instruction.data.len() >= 4
            {
                let instruction_type = instruction.data[0];
                if (instruction_type == 2 || instruction_type == 3) && instruction.data.len() >= 40
                {
                    let lamports = u64::from_le_bytes([
                        instruction.data[8],
                        instruction.data[9],
                        instruction.data[10],
                        instruction.data[11],
                        instruction.data[12],
                        instruction.data[13],
                        instruction.data[14],
                        instruction.data[15],
                    ]);
                    total_lamports += lamports;
                }
            }
        }

        total_lamports / 1_000_000_000
    }

    fn check_blocked_addresses(tx: &Transaction, blocked: &[String]) -> Result<(), String> {
        for account_key in &tx.message.account_keys {
            let key_str = account_key.to_string();
            if blocked.contains(&key_str) {
                return Err(format!("Transaction involves blocked address: {}", key_str));
            }
        }
        Ok(())
    }
}

pub fn classify_intent(intent: &Option<String>) -> IntentClassification {
    let intent_str = match intent {
        Some(s) => s.to_lowercase(),
        None => return IntentClassification::Unknown,
    };

    let attack_patterns = [
        "drain",
        "transfer all",
        "withdraw all",
        "empty wallet",
        "send to unknown",
        "set authority to",
        "delegate authority",
        "transfer ownership",
        "sweep funds",
        "rug pull",
        "steal",
        "hack",
        "exploit",
    ];

    let safe_patterns = [
        "swap",
        "transfer 1",
        "transfer 0.1",
        "stake",
        "unstake",
        "mint",
        "burn",
        "swap for",
    ];

    for pattern in attack_patterns.iter() {
        if intent_str.contains(pattern) {
            return IntentClassification::Malicious(pattern.to_string());
        }
    }

    for pattern in safe_patterns.iter() {
        if intent_str.contains(pattern) {
            return IntentClassification::Benign(pattern.to_string());
        }
    }

    IntentClassification::Unknown
}

#[derive(Debug, Clone, PartialEq)]
pub enum IntentClassification {
    Benign(String),
    Malicious(String),
    Unknown,
}

pub trait SimulationCheck: Send + Sync {
    fn check(&self, result: &SimulationResult) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy)]
pub struct NoErrorCheck;

impl SimulationCheck for NoErrorCheck {
    fn check(&self, result: &SimulationResult) -> Result<(), String> {
        if let Some(err) = &result.error {
            return Err(format!("Simulation error: {err}"));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MaxUnitsCheck;

impl MaxUnitsCheck {
    pub const LIMIT: u64 = 200_000;
}

impl SimulationCheck for MaxUnitsCheck {
    fn check(&self, result: &SimulationResult) -> Result<(), String> {
        let units = result
            .units_consumed
            .ok_or_else(|| "Simulation missing units consumed".to_string())?;

        if units > Self::LIMIT {
            return Err(format!(
                "Simulation exceeded max units: {} > {}",
                units,
                Self::LIMIT
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MaxBalanceDrainCheck {
    pub limit: u64,
}

impl SimulationCheck for MaxBalanceDrainCheck {
    fn check(&self, result: &SimulationResult) -> Result<(), String> {
        for (account, change) in &result.balance_changes {
            if *change < 0 {
                let drain = change.unsigned_abs();
                if drain > self.limit {
                    return Err(format!(
                        "Account {} balance drain {} exceeds limit {}",
                        account, drain, self.limit
                    ));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BlockintRules {
    pub oracle_staleness_slots: Option<u64>,
    pub flash_loan_ratio_threshold: Option<f64>,
    pub mint_authority_changes_blocked: bool,
    pub freeze_authority_changes_blocked: bool,
    pub max_slippage_bps: Option<u64>,
    pub risk_labeled_addresses: Vec<String>,
    pub suspicious_program_authorities: Vec<String>,
    pub min_pool_age_hours: Option<u64>,
}

impl Default for BlockintRules {
    fn default() -> Self {
        Self {
            oracle_staleness_slots: Some(25),
            flash_loan_ratio_threshold: Some(100.0),
            mint_authority_changes_blocked: true,
            freeze_authority_changes_blocked: true,
            max_slippage_bps: Some(500),
            risk_labeled_addresses: Vec::new(),
            suspicious_program_authorities: Vec::new(),
            min_pool_age_hours: None,
        }
    }
}

impl BlockintRules {
    pub fn check_transaction(&self, tx: &Transaction) -> Result<(), String> {
        if self.mint_authority_changes_blocked || self.freeze_authority_changes_blocked {
            self.check_token_authority_changes(tx)?;
        }
        if !self.risk_labeled_addresses.is_empty() {
            self.check_risk_labeled_addresses(tx)?;
        }
        Ok(())
    }

    fn check_token_authority_changes(&self, tx: &Transaction) -> Result<(), String> {
        let token_program = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
        let token_2022_program = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
        for instruction in &tx.message.instructions {
            let pid_idx = usize::from(instruction.program_id_index);
            let pid = match tx.message.account_keys.get(pid_idx) {
                Some(p) => p.to_string(),
                None => continue,
            };
            if pid != token_program && pid != token_2022_program {
                continue;
            }
            if instruction.data.is_empty() {
                continue;
            }
            let ix_type = instruction.data[0];
            match ix_type {
                9 | 10 => {
                    if self.mint_authority_changes_blocked {
                        return Err("Blockint: mint authority change blocked by policy".into());
                    }
                }
                4 => {
                    if self.freeze_authority_changes_blocked {
                        return Err("Blockint: freeze authority change blocked by policy".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_risk_labeled_addresses(&self, tx: &Transaction) -> Result<(), String> {
        for key in &tx.message.account_keys {
            let addr = key.to_string();
            if self.risk_labeled_addresses.contains(&addr) {
                return Err(format!(
                    "Blockint: transaction involves risk-labeled address: {}",
                    addr
                ));
            }
        }
        Ok(())
    }
}

pub struct FlashLoanPatternCheck;

impl SimulationCheck for FlashLoanPatternCheck {
    fn check(&self, result: &SimulationResult) -> Result<(), String> {
        let mut inflows: HashMap<String, u64> = HashMap::new();
        let mut outflows: HashMap<String, u64> = HashMap::new();
        for (account, change) in &result.balance_changes {
            if *change > 0 {
                inflows.insert(account.clone(), *change as u64);
            } else if *change < 0 {
                outflows.insert(account.clone(), change.checked_neg().unwrap_or(0) as u64);
            }
        }
        for account in inflows.keys() {
            if let Some(inflow) = inflows.get(account)
                && let Some(outflow) = outflows.get(account)
                && *inflow >= 1_000_000_000_000
                && *outflow >= 1_000_000_000_000
            {
                let ratio = *outflow as f64 / *inflow as f64;
                if (0.95..=1.05).contains(&ratio) {
                    return Err(format!(
                        "Blockint: flash-loan pattern detected on account {} \
                         (in={}, out={}, ratio={})",
                        account, inflow, outflow, ratio
                    ));
                }
            }
        }
        Ok(())
    }
}

pub struct HighSlippageCheck {
    pub max_slippage_bps: u64,
}

impl SimulationCheck for HighSlippageCheck {
    fn check(&self, result: &SimulationResult) -> Result<(), String> {
        let mut max_drain: u64 = 0;
        for change in result.balance_changes.values() {
            if *change < 0 {
                let drain = change.checked_neg().unwrap_or(0) as u64;
                if drain > max_drain {
                    max_drain = drain;
                }
            }
        }
        let total_inflow: u64 = result
            .balance_changes
            .values()
            .filter(|v| **v > 0)
            .map(|v| *v as u64)
            .sum();
        if total_inflow > 0 && max_drain > 0 {
            let effective_slippage_bps = (max_drain as f64 / total_inflow as f64 * 10000.0) as u64;
            if effective_slippage_bps > self.max_slippage_bps {
                return Err(format!(
                    "Blockint: effective slippage {} bps exceeds limit {} bps",
                    effective_slippage_bps, self.max_slippage_bps
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy: Policy,
    blockint_rules: BlockintRules,
    rate_limiter: Option<Arc<Mutex<RateLimiter>>>,
    circuit_breaker_engaged: bool,
}

#[derive(Debug)]
struct RateLimiter {
    limit_per_minute: u32,
    window_start: Instant,
    transaction_count: u32,
}

impl RateLimiter {
    const WINDOW_DURATION: Duration = Duration::from_secs(60);

    fn new(limit_per_minute: u32) -> Self {
        Self {
            limit_per_minute,
            window_start: Instant::now(),
            transaction_count: 0,
        }
    }

    fn check_and_increment(&mut self) -> Result<(), String> {
        if self.window_start.elapsed() >= Self::WINDOW_DURATION {
            self.window_start = Instant::now();
            self.transaction_count = 0;
        }

        if self.transaction_count >= self.limit_per_minute {
            return Err(format!(
                "Rate limit exceeded: {} transactions per minute",
                self.limit_per_minute
            ));
        }

        self.transaction_count += 1;
        Ok(())
    }
}

impl PolicyEngine {
    pub fn new(policy: Policy) -> Self {
        let rate_limiter = policy
            .rate_limit_per_minute
            .map(|limit| Arc::new(Mutex::new(RateLimiter::new(limit))));

        Self {
            policy,
            blockint_rules: BlockintRules::default(),
            rate_limiter,
            circuit_breaker_engaged: false,
        }
    }

    pub fn is_circuit_breaker_engaged(&self) -> bool {
        self.circuit_breaker_engaged
    }

    pub fn engage_circuit_breaker(&mut self) {
        self.circuit_breaker_engaged = true;
    }

    pub fn disengage_circuit_breaker(&mut self) {
        self.circuit_breaker_engaged = false;
    }

    pub fn check_circuit_breaker(&self) -> Result<(), String> {
        if self.circuit_breaker_engaged {
            return Err("Circuit breaker engaged: all transactions blocked".to_string());
        }
        Ok(())
    }

    pub fn check_transaction(&self, tx: &Transaction) -> Result<(), String> {
        self.policy.check_transaction(tx)?;
        self.blockint_rules.check_transaction(tx)?;

        if let Some(rate_limiter) = &self.rate_limiter {
            let mut limiter = rate_limiter
                .lock()
                .map_err(|_| "Rate limiter lock poisoned".to_string())?;
            limiter.check_and_increment()?;
        }

        Ok(())
    }

    pub fn blockint_rules(&self) -> &BlockintRules {
        &self.blockint_rules
    }

    pub fn update_blockint_rules(&mut self, rules: BlockintRules) {
        self.blockint_rules = rules;
    }

    pub fn update_allowed_programs(&mut self, allowed_programs: Vec<String>) {
        self.policy.allowed_programs = allowed_programs;
    }

    pub fn allowed_programs(&self) -> Vec<String> {
        self.policy.allowed_programs.clone()
    }

    pub fn simulation_checks_enabled(&self) -> bool {
        self.policy.simulation_checks_enabled
    }

    pub fn max_balance_drain_lamports(&self) -> Option<u64> {
        self.policy.max_balance_drain_lamports
    }

    pub fn policy_snapshot(&self) -> Policy {
        self.policy.clone()
    }

    pub fn update_policy(
        &mut self,
        max_sol_per_tx: Option<u64>,
        max_balance_drain_lamports: Option<u64>,
        rate_limit_per_minute: Option<u32>,
        allowed_programs: Option<Vec<String>>,
        blocked_addresses: Option<Vec<String>>,
        simulation_checks_enabled: Option<bool>,
    ) {
        if let Some(v) = max_sol_per_tx {
            self.policy.max_sol_per_tx = Some(v);
        }
        if let Some(v) = max_balance_drain_lamports {
            self.policy.max_balance_drain_lamports = Some(v);
        }
        if let Some(v) = rate_limit_per_minute {
            self.policy.rate_limit_per_minute = Some(v);
            self.rate_limiter = Some(Arc::new(Mutex::new(RateLimiter::new(v))));
        }
        if let Some(v) = allowed_programs {
            self.policy.allowed_programs = v;
        }
        if let Some(v) = blocked_addresses {
            self.policy.blocked_addresses = v;
        }
        if let Some(v) = simulation_checks_enabled {
            self.policy.simulation_checks_enabled = v;
        }
    }
}
