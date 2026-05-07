# Sentinel Guard 🛡️

**Autonomous AI Agents need a Firewall. Sentinel is that firewall.**

Sentinel Guard is a high-performance Rust security middleware designed for autonomous AI agents on Solana. It acts as a deterministic barrier between an agent's non-deterministic logic and its wallet, ensuring that every transaction aligns with human-defined safety policies before it's signed and broadcast to the network.

## 🚀 The Core Problem
AI Agents are powerful but unpredictable. They are susceptible to:
- **Prompt Injection:** Attackers can trick agents into sending funds to malicious addresses.
- **Slippage & MEV:** Agents might execute sub-optimal swaps without proper protection.
- **Shadow Delegations:** Malicious programs could trick an agent into delegating token authority.

## 🛠️ The Solution
Sentinel intercepts transaction requests, simulates them via the Helius Simulation API, and evaluates the outcome against a multi-stage policy engine.

### Key Features
- **Deterministic Policy Engine:** Whitelist specific programs (e.g., Jupiter, System Program), set per-transaction SOL caps, and enforce rate limits.
- **Simulation-Based Verification:** Inspects actual state changes—balance drops, authority shifts, and compute units—before the transaction is signed.
- **Human-in-the-Loop Override:** A real-time web dashboard for manual approval of "suspicious" but potentially valid transactions.
- **Audit Logging:** Every decision, simulation result, and reasoning is persisted to an embedded `sled` database.

## 🏗️ Architecture
1. **The Interceptor (Axum):** A high-speed Rust proxy that presents a simple API for agents to submit transactions for validation.
2. **The Simulation Core:** Integrates with Helius Simulation API to forecast the exact outcome of instructions.
3. **The Policy Engine:** Executes a multi-stage check:
    - **Static:** Whitelist verification.
    - **Simulation:** Balance drain and compute unit caps.
    - **Behavioral:** Rate limiting and intent logging.
4. **The Dashboard:** A Tailwind-powered UI for real-time monitoring and intervention.

## 🚦 Getting Started

### Prerequisites
- Rust (Stable)
- Helius API Key (Required for simulation features)

### Installation
```bash
git clone https://github.com/ClawdieRS/sentinel
cd sentinel
# Add your HELIUS_API_KEY to your environment
export HELIUS_API_KEY="your-api-key-here"
cargo build --release
```

### Configuration (`config.toml`)
Customize your security parameters:
```toml
max_sol_per_tx = 1
max_balance_drain_lamports = 100000000 # 0.1 SOL cap
rate_limit_per_minute = 10
allowed_programs = [
    "11111111111111111111111111111111", # System Program
    "TokenkSzhZwpDfbvXPB9SSct59MSBhGUMCfX2LzXBe", # Token Program
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"  # Jupiter v6
]
simulation_checks_enabled = true
```

## 🔌 API Reference

### `POST /simulate`
Submit a base64-encoded transaction for validation.
- **Payload:** `{ "transaction": "base64_tx", "intent": "Optional description" }`
- **Success:** Returns simulation results if allowed.
- **Failure:** Returns `403 Forbidden` with a `block_id` for human override.

### `POST /override`
Manual approval/rejection via the dashboard.
- **Payload:** `{ "block_id": "uuid", "action": "ALLOW|REJECT" }`

### `GET /logs`
Fetch persisted audit entries from sled with optional filters.
- Query params:
  - `result=ALLOWED|BLOCKED`
  - `transaction_id=<hash>`
  - `signature=<tx_signature>`
  - `offset=<n>` and `limit=<n>` (default limit `100`)

### `GET /logs/tx/{transaction_id}`
Fetch audit entries for a specific transaction hash.

### `GET /logs/signature/{signature}`
Fetch audit entries for a specific Solana signature.

### `GET /audit/logs`
Detailed audit alias for `GET /logs` with the same query support.

### `GET /audit/logs/tx/{transaction_id}`
Detailed audit alias for `GET /logs/tx/{transaction_id}`.

### `GET /audit/logs/signature/{signature}`
Detailed audit alias for `GET /logs/signature/{signature}`.

### `GET /policy`
Return the current dynamic allowlist policy.

### `PUT /policy` (or `POST /policy`)
Update `allowed_programs` at runtime.
- **Payload:** `{ "allowed_programs": ["program_id_1", "program_id_2"] }`

### `PUT /policy/allowed-programs` (or `POST /policy/allowed-programs`)
Detailed policy alias for updating `allowed_programs` at runtime.

## 📊 Dashboard
The Sentinel dashboard is available at `http://localhost:3000/dashboard`. 
It provides a live feed of agent activity and an interface for resolving pending security alerts.

---
Built with 🦀 by **ClawdieLabs** for the Colosseum Hackathon.
