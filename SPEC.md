# Sentinel: The AI Agent Firewall 🛡️

**Tagline:** "Trust your Agent, but Verify the Transaction."

## The Problem
AI Agents are non-deterministic. They hallucinate. Prompt injection attacks can force them to sign malicious transactions.
Currently, agents sign whatever the LLM outputs. This is a massive security risk.

## The Solution
**Sentinel** is a Rust-based middleware that sits between the Agent's "Brain" (LLM) and the "Wallet" (Signing).
It intercepts every transaction request, **Simulates** it against a local fork (or Helius simulation API), and checks it against a strict **Policy File**.

## Core Architecture

1.  **Policy Engine (The Rulebook)**
    *   Written in TOML/JSON (Human readable).
    *   Rules: `MaxSpendPerTx`, `AllowedPrograms`, `WhitelistAddresses`, `RateLimit`.

2.  **The Interceptor (Rust Proxy)**
    *   A local server (localhost:3000) that looks like a standard Solana RPC.
    *   The Agent sends transactions here instead of mainnet.

3.  **Simulation Core (The Truth)**
    *   Decodes the instruction data.
    *   Simulates the state change (Balance change? Token delegation?).
    *   **Crucial:** Checks for "Drain" patterns (e.g., `SetAuthority` to unknown address).

4.  **The Gatekeeper**
    *   If PASS: Signs and forwards to Jito/RPC.
    *   If FAIL: Returns error to Agent ("Blocked by Sentinel: Policy Violation").

## Tech Stack
*   **Language:** Rust (for speed & safety).
*   **Framework:** Axum (Web server) + Solana SDK.
*   **Simulation:** Helius API or Local Bankrun (for speed).
*   **Database:** Sled (embedded Rust DB) for audit logs.

## Hackathon Strategy
1.  **Day 1-2:** Build the Rust Proxy & Policy Parser.
2.  **Day 3-4:** Integrate Simulation (Transaction parsing).
3.  **Day 5:** Build a "Test Agent" that tries to hack itself.
4.  **Day 6-10:** Polish & UI Dashboard.

**Why this wins:** It's a "Pick and Shovel" play. Every other agent needs this.
