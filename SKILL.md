---
name: sentinelguard
description: "High-performance Rust firewall for AI Agents. Intercepts, simulates, and validates transactions before signing."
homepage: "https://github.com/ClawdieLabs/sentinel"
metadata: {
  "category": "security",
  "emoji": "🛡️",
  "requires": {
    "bins": ["sentinelguard"]
  }
}
---

# SentinelGuard 🛡️

SentinelGuard is an autonomous security middleware that sits between an Agent's Brain and its Wallet.

## Installation

```bash
# Clone and build
git clone https://github.com/ClawdieLabs/sentinel.git
cd sentinel && cargo build --release
```

## Features
- **Transaction Simulation**: Uses Helius simulation API to predict balance changes.
- **Program Whitelisting**: Blocks unauthorized Program IDs.
- **Audit Logging**: Persistent history of all attempts via Sled DB.
- **REST API**: Dynamically update policies and fetch logs.
- **Human-in-the-loop**: Request manual approval for suspicious transactions.

## Usage for Agents

Other Clawdbot agents should use SentinelGuard to verify every transaction before sending it to the network.

### Verification Flow
1. **Prepare Transaction**: Build your Solana transaction.
2. **Simulate/Verify**: Send `POST /simulate` with the base64 transaction and your `intent` (what you think you are doing).
3. **Handle Response**:
    - **200 OK**: Success! Proceed to sign and broadcast.
    - **403 Forbidden**: Check for `block_id` in the response.
        - If `block_id` exists: The transaction is suspicious but might be valid. **You MUST notify the human**, explain the intent, show the block reason, and provide the `block_id` for them to decide.
        - If no `block_id`: The transaction is explicitly denied by static policy. Abort.

### Including Intent
Always provide a human-readable `intent` string in your requests. This is logged and shown to the human during override requests.
Example: `"intent": "Swapping 1 SOL for USDC on Jupiter"`

### Handling Blocks (Human Override)
If a transaction is blocked with a `block_id`, ask the human:
"SentinelGuard blocked this transaction: [Reason]. If you trust this, reply with 'allow [block_id]' or 'reject [block_id]'."

Then call `POST /override` with:
```json
{
  "block_id": "...",
  "action": "ALLOW" or "REJECT"
}
```

## API Endpoints
- `POST /simulate`: Intercept and verify a transaction.
- `GET /logs`: Fetch audit history.
- `POST /policy`: Update allowed programs list.
- `POST /override`: Human override for a blocked transaction.
