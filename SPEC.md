# Bastion v2: AI Agent Firewall 🛡️

**Tagline:** "Trust your Agent, but Verify every Transaction."

## The Problem
AI Agents are non-deterministic. They hallucinate. Prompt injection attacks can force them to sign malicious transactions.
Currently, agents sign whatever the LLM outputs. This is a massive security risk.

## The Solution
**Bastion** is a Rust-based middleware that sits between the Agent's "Brain" (LLM) and the "Wallet" (Signing).
It intercepts every transaction request, **Simulates** it, and checks it against a strict **Policy Engine**.

## v2 New Features (vs Sentinel v1)

### 1. On-Chain Audit Program
- Anchor program for immutable audit records on Solana
- PDA-based audit log storage
- Verifiable reputation for agents
- Program allowlist managed on-chain

### 2. Agent Identity Registry
- On-chain agent registration
- Capability bitmasks
- Stake-weighted reputation

### 3. Multi-Agent Support
- Session key management per agent
- Agent-specific policies
- Per-agent rate limits

### 4. Enhanced Policy Engine
- Time-based policy windows
- Geographic/frequency-based throttling
- Emergency circuit breaker

## Core Architecture

1.  **Policy Engine (The Rulebook)**
    *   Written in TOML/JSON (Human readable).
    *   Rules: `MaxSpendPerTx`, `AllowedPrograms`, `WhitelistAddresses`, `RateLimit`.
    *   On-chain policy PDA for immutable rules.

2.  **The Interceptor (Rust Proxy)**
    *   A local server (localhost:3000) that looks like a standard Solana RPC.
    *   The Agent sends transactions here instead of mainnet.

3.  **Simulation Core (The Truth)**
    *   Decodes the instruction data.
    *   Simulates the state change (Balance change? Token delegation?).
    *   Checks for "Drain" patterns (e.g., `SetAuthority` to unknown address).

4.  **On-Chain Audit (Anchor Program)**
    *   Immutable audit trail on Solana
    *   Verifiable decision records
    *   Agent reputation tracking

5.  **The Gatekeeper**
    *   If PASS: Signs and forwards to Jito/RPC.
    *   If FAIL: Returns error to Agent ("Blocked by Bastion: Policy Violation").

## Tech Stack
- **Language:** Rust (for speed & safety).
- **Framework:** Axum (Web server) + Solana SDK.
- **Simulation:** Helius API or Local Bankrun.
- **Database:** Sled (embedded Rust DB) for local audit logs.
- **On-Chain:** Anchor (Solana programs).

## Roadmap

### Phase 1: Core Interceptor (Day 1-2)
- [x] Rust Proxy & Policy Parser (from Sentinel)
- [x] Transaction validation
- [ ] Policy API improvements

### Phase 2: On-Chain Audit (Day 3-4)
- [ ] Anchor program structure
- [ ] PDA-based audit storage
- [ ] CPI from interceptor to program

### Phase 3: Agent Registry (Day 5-6)
- [ ] On-chain agent registration
- [ ] Capability bitmasks
- [ ] Reputation tracking

### Phase 4: Dashboard (Day 7-8)
- [ ] Real-time policy editor
- [ ] Transaction feed + alerts
- [ ] Agent status overview

### Phase 5: Advanced Security (Day 9-10)
- [ ] Prompt injection detection
- [ ] Rate limiting per agent
- [ ] Anomaly detection hooks
- [ ] Emergency circuit breaker

## Why This Wins
It's a "Pick and Shovel" play. Every autonomous agent needs security.
This is infrastructure that every AI agent builder will need.