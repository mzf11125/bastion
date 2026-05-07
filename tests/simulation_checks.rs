//! Phase 5 — Simulation-check unit tests.
//!
//! Verifies `NoErrorCheck`, `MaxUnitsCheck`, and `MaxBalanceDrainCheck` in
//! isolation, covering boundary values and multiple-account scenarios.

use sentinel::policy::{MaxBalanceDrainCheck, MaxUnitsCheck, NoErrorCheck, SimulationCheck};
use sentinel::simulation::SimulationResult;
use std::collections::HashMap;

fn clean_result() -> SimulationResult {
    SimulationResult {
        logs: vec![],
        units_consumed: Some(100_000),
        return_data: None,
        error: None,
        balance_changes: HashMap::new(),
    }
}

// ── NoErrorCheck ─────────────────────────────────────────────────────────

#[test]
fn no_error_check_passes_when_error_is_none() {
    let result = clean_result();
    assert!(NoErrorCheck.check(&result).is_ok());
}

#[test]
fn no_error_check_fails_when_error_is_present() {
    let mut result = clean_result();
    result.error = Some(serde_json::json!({"InstructionError": [0, {"Custom": 6001}]}));
    let err = NoErrorCheck.check(&result).expect_err("should fail");
    assert!(err.contains("Simulation error"), "got: {err}");
}

#[test]
fn no_error_check_fails_on_string_error() {
    let mut result = clean_result();
    result.error = Some(serde_json::json!("AccountNotFound"));
    let err = NoErrorCheck.check(&result).expect_err("should fail");
    assert!(err.contains("AccountNotFound"), "got: {err}");
}

// ── MaxUnitsCheck ────────────────────────────────────────────────────────

#[test]
fn max_units_check_passes_exactly_at_limit() {
    let mut result = clean_result();
    result.units_consumed = Some(MaxUnitsCheck::LIMIT);
    assert!(MaxUnitsCheck.check(&result).is_ok());
}

#[test]
fn max_units_check_fails_one_over_limit() {
    let mut result = clean_result();
    result.units_consumed = Some(MaxUnitsCheck::LIMIT + 1);
    let err = MaxUnitsCheck.check(&result).expect_err("should fail");
    assert!(err.contains("exceeded max units"), "got: {err}");
}

#[test]
fn max_units_check_passes_well_under_limit() {
    let mut result = clean_result();
    result.units_consumed = Some(1);
    assert!(MaxUnitsCheck.check(&result).is_ok());
}

#[test]
fn max_units_check_fails_when_units_consumed_is_none() {
    let mut result = clean_result();
    result.units_consumed = None;
    let err = MaxUnitsCheck.check(&result).expect_err("should fail");
    assert!(err.contains("missing units consumed"), "got: {err}");
}

#[test]
fn max_units_check_passes_zero_units() {
    let mut result = clean_result();
    result.units_consumed = Some(0);
    assert!(MaxUnitsCheck.check(&result).is_ok());
}

// ── MaxBalanceDrainCheck ─────────────────────────────────────────────────

#[test]
fn balance_drain_check_passes_when_no_changes() {
    let result = clean_result();
    let check = MaxBalanceDrainCheck { limit: 1_000_000 };
    assert!(check.check(&result).is_ok());
}

#[test]
fn balance_drain_check_passes_when_drain_exactly_at_limit() {
    let mut result = clean_result();
    result
        .balance_changes
        .insert("Acct1".to_string(), -1_000_000);
    let check = MaxBalanceDrainCheck { limit: 1_000_000 };
    assert!(check.check(&result).is_ok());
}

#[test]
fn balance_drain_check_fails_when_drain_exceeds_limit() {
    let mut result = clean_result();
    result
        .balance_changes
        .insert("Acct1".to_string(), -1_000_001);
    let check = MaxBalanceDrainCheck { limit: 1_000_000 };
    let err = check.check(&result).expect_err("should fail");
    assert!(err.contains("balance drain"), "got: {err}");
    assert!(err.contains("Acct1"), "got: {err}");
}

#[test]
fn balance_drain_check_ignores_positive_changes() {
    let mut result = clean_result();
    result
        .balance_changes
        .insert("Acct1".to_string(), 999_999_999);
    let check = MaxBalanceDrainCheck { limit: 100 };
    assert!(check.check(&result).is_ok());
}

#[test]
fn balance_drain_check_catches_any_account_over_limit() {
    let mut result = clean_result();
    result.balance_changes.insert("Safe".to_string(), -500);
    result
        .balance_changes
        .insert("Drainer".to_string(), -2_000_000);
    result
        .balance_changes
        .insert("Receiver".to_string(), 3_000_000);
    let check = MaxBalanceDrainCheck { limit: 1_000_000 };
    let err = check.check(&result).expect_err("should fail");
    assert!(err.contains("Drainer"), "got: {err}");
}

#[test]
fn balance_drain_check_passes_multiple_small_drains_under_limit() {
    let mut result = clean_result();
    result.balance_changes.insert("A".to_string(), -100);
    result.balance_changes.insert("B".to_string(), -200);
    result.balance_changes.insert("C".to_string(), -300);
    let check = MaxBalanceDrainCheck { limit: 500 };
    assert!(check.check(&result).is_ok());
}

#[test]
fn balance_drain_check_zero_limit_blocks_any_drain() {
    let mut result = clean_result();
    result.balance_changes.insert("Acct".to_string(), -1);
    let check = MaxBalanceDrainCheck { limit: 0 };
    let err = check.check(&result).expect_err("should fail");
    assert!(err.contains("balance drain"), "got: {err}");
}
