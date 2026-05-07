use crate::simulation::SimulationResult;
use serde::{Deserialize, Serialize};
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

        Ok(())
    }
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
                let drain = change.abs() as u64;
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
pub struct PolicyEngine {
    policy: Policy,
    rate_limiter: Option<Arc<Mutex<RateLimiter>>>,
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
            rate_limiter,
        }
    }

    pub fn check_transaction(&self, tx: &Transaction) -> Result<(), String> {
        self.policy.check_transaction(tx)?;

        if let Some(rate_limiter) = &self.rate_limiter {
            let mut limiter = rate_limiter
                .lock()
                .map_err(|_| "Rate limiter lock poisoned".to_string())?;
            limiter.check_and_increment()?;
        }

        Ok(())
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
