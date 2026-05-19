# Bastion ↔ Grond Integration

Bastion uses Grond's OSINT pipeline as a risk oracle for blockchain addresses.

## How it works

```
Agent tx → Bastion Sidecar → GrondOracle ──HTTP──→ Grond FastAPI
                                       ↓                    ↓
                              RiskScore(0-100)    Tavily/Shodan/Twitter search
```

When `GROND_API_URL` is set, every transaction through Bastion's v2 evaluate
endpoint triggers a Grond OSINT sweep on the destination address.

## Setup

### 1. In the Grond repo

Copy `docs/grond-integration/address_risk.py` into `src/tools/`:

```bash
cp docs/grond-integration/address_risk.py ../Grond/src/tools/address_risk.py
```

Register the route in `src/api/main.py`:

```python
from src.tools.address_risk import address_risk_endpoint, AddressRiskInput

@app.post("/api/v1/tools/address-risk")
async def address_risk(inp: AddressRiskInput):
    audit = get_audit_logger()  # your audit logger singleton
    return await address_risk_endpoint(inp, audit=audit)
```

### 2. In Bastion

Set the environment variable:

```bash
export GROND_API_URL=http://localhost:8000
```

Start Bastion — it will auto-detect and enable the GrondOSINT oracle.

## Configuration

| Env var | Default | Description |
|---------|---------|-------------|
| `GROND_API_URL` | (unset) | Grond FastAPI base URL (e.g. `http://localhost:8000`) |

When unset, the oracle is disabled and Bastion behaves as before.

## Risk scoring

Grond searches Tavily for scam/rug/hack/phishing keywords associated with
the destination address. Each keyword carries a weight, and results are
weighted by Tavily's confidence score.

| Risk Level | Score Range | Behavior |
|-----------|-------------|----------|
| LOW | 0-25 | Transaction passes |
| MEDIUM | 26-60 | Flagged for HITL (if reputation policy configured) |
| HIGH | 61-100 | Blocked by default |
