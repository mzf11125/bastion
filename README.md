# Bastion - AI Agent Firewall for Solana

Bastion is a high-performance security middleware for autonomous AI agents on Solana. It acts as a deterministic barrier between an agent's non-deterministic logic and its wallet, ensuring that every transaction aligns with human-defined safety policies before being signed and broadcast to the network.

## Table of Contents

- [Overview](#overview)
- [Problem](#problem)
- [Solution](#solution)
- [Features](#features)
- [Architecture](#architecture)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [On-Chain Program](#on-chain-program)
- [Dashboard](#dashboard)
- [SDK](#sdk)
- [Tech Stack](#tech-stack)
- [Contributing](#contributing)
- [License](#license)

## Overview

Bastion v2 is an upgraded fork of Sentinel, built for the Solana Frontier Hackathon Infrastructure track. It provides:

- Transaction validation and simulation
- Policy-based access control
- On-chain audit trail
- Agent identity registry
- Real-time dashboard

## Problem

AI agents are powerful but unpredictable. They are susceptible to:

- **Prompt Injection**: Attackers trick agents into malicious transactions
- **Balance Drain**: Unchecked transfers drain wallets
- **Unauthorized Programs**: Malicious program calls
- **No Audit Trail**: No way to verify agent behavior

## Solution

Bastion intercepts transaction requests, simulates them via Helius Simulation API, and evaluates against a multi-stage policy engine.

## Features

| Feature | Description |
|---------|-------------|
| Policy Engine | Program whitelist, SOL caps, rate limits |
| Transaction Simulation | State change prediction via Helius |
| Human-in-the-Loop | Manual override for suspicious tx |
| Audit Logging | Sled DB for local audit records |
| On-Chain Audit | Anchor program for immutable records |
| Agent Registry | On-chain agent identity + reputation |
| Emergency Pause | Circuit breaker for protocol |

## Architecture

Bastion consists of five main components:

1. **Interceptor (Axum)**: Rust HTTP proxy for transaction validation
2. **Simulation Core**: Helius API integration for outcome prediction
3. **Policy Engine**: Static (whitelist), Simulation (balance check), Behavioral (rate limit)
4. **On-Chain Audit Program**: Anchor program for immutable records
5. **Dashboard**: React+Vite UI for monitoring and policy management

## Quick Start

### Prerequisites

- Rust (stable)
- Node.js 18+
- Helius API Key (for simulation)

### Build and Run

```bash
# Clone the repository
git clone https://github.com/mzf11125/bastion.git
cd bastion

# Build the middleware
cargo build --release

# Set Helius API key
export HELIUS_API_KEY="your-api-key"

# Run the server
cargo run --release

# Server starts at http://localhost:3000
# Dashboard at http://localhost:3000/dashboard
```

### Run the Dashboard

```bash
cd dashboard
npm install
npm run dev
# Dashboard opens at http://localhost:3000
```

### Use the SDK

```bash
cd sdk
npm install
```

```typescript
import { BastionClient, AGENT_CAPABILITIES } from "@bastion/sdk";

const client = new BastionClient({
  connection: new Connection("https://api.devnet.solana.com")
});

// Register an agent
const tx = await client.registerAgent(
  wallet,
  "MyTradingBot",
  AGENT_CAPABILITIES.TRANSFER | AGENT_CAPABILITIES.SWAP
);
```

## Configuration

Create `config.toml` in the project root:

```toml
# Policy Settings
max_sol_per_tx = 1
max_balance_drain_lamports = 100000000  # 0.1 SOL
rate_limit_per_minute = 10
simulation_checks_enabled = true

# Allowed Programs (whitelist mode)
allowed_programs = [
    "11111111111111111111111111111111",  # System Program
    "TokenkegZwpDfbvXPB9SSct59MSBhGUMCfX2LzXBe",  # Token Program
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",  # Jupiter v6
]

# Blocked Addresses
blocked_addresses = []
```

## API Reference

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | /simulate | Validate transaction |
| GET | /logs | Fetch audit logs |
| GET | /policy | Get policy settings |
| PUT | /policy | Update policy |
| POST | /override | Human override |
| GET | /health | Health check |

### POST /simulate

Validate a transaction before signing.

```bash
curl -X POST http://localhost:3000/simulate \
  -H "Content-Type: application/json" \
  -d '{"transaction": "base64_encoded_tx", "intent": "Swap 1 SOL for USDC"}'
```

**Success Response (200)**:
```json
{
  "units_consumed": 150000,
  "balance_changes": {"wallet": -1000000000}
}
```

**Blocked Response (403)**:
```json
{
  "error": "Exceeds max SOL per transaction",
  "block_id": "uuid-for-override"
}
```

### GET /logs

Fetch audit history.

```bash
curl "http://localhost:3000/logs?limit=10&offset=0"
```

### POST /override

Override a blocked transaction.

```bash
curl -X POST http://localhost:3000/override \
  -H "Content-Type: application/json" \
  -d '{"block_id": "uuid", "action": "ALLOW"}'
```

## On-Chain Program

The Anchor program provides immutable audit records on Solana.

### Program ID

```
BaStion11111111111111111111111111111111
```

### Instructions

| Instruction | Description |
|-------------|-------------|
| initialize | Initialize audit state |
| log_audit | Record transaction audit |
| register_agent | Register agent on-chain |
| update_agent_reputation | Update agent reputation |
| set_policy | Set on-chain policy |
| emergency_pause | Pause protocol |
| emergency_resume | Resume protocol |

### Build and Deploy

```bash
cd programs/bastion-audit
anchor build
anchor deploy --provider.cluster devnet
```

## Dashboard

The dashboard provides real-time monitoring and policy management.

### Features

- Live transaction feed
- Pending approval queue
- Audit logs viewer
- Policy editor
- Emergency pause/resume
- Statistics (total/allowed/blocked)

### Run

```bash
cd dashboard
npm install
npm run dev
```

## SDK

TypeScript SDK for programmatic access.

### Installation

```bash
npm install @bastion/sdk
```

### Usage

```typescript
import { BastionClient, AGENT_CAPABILITIES } from "@bastion/sdk";

const client = new BastionClient({
  connection: new Connection("https://api.devnet.solana.com")
});

// Register agent
await client.registerAgent(wallet, "MyBot", AGENT_CAPABILITIES.TRANSFER);

// Set policy
await client.setPolicy(wallet, [jupiterProgram], 5, 10);

// Emergency pause
await client.emergencyPause(wallet);
```

## Tech Stack

| Component | Technology |
|-----------|-------------|
| Middleware | Rust, Axum |
| Simulation | Helius API |
| Database | Sled |
| On-Chain | Anchor, Solana SDK |
| SDK | TypeScript |
| Dashboard | React, Vite, TailwindCSS |

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Submit a pull request

## License

MIT License - See LICENSE file for details.

---

Built for the Solana Frontier Hackathon by Bastion Defend.