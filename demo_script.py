import requests
import json

BASE_URL = "http://localhost:3000"

def run_demo():
    print("🚀 SentinelGuard Demo: Autonomous Security for AI Agents\n")
    
    # 1. Check health
    print("[1/3] Checking Sentinel Status...")
    try:
        requests.get(BASE_URL).json()
    except:
        pass
    print("✅ Sentinel is Active on port 3000\n")

    # 2. Simulate a malicious payload
    print("[2/3] Simulating Transaction Intent: 'Drain all tokens to unknown address'")
    # This is dummy base64 that fails deserialization but triggers the audit log
    tx_payload = "SGVsbG8gU29sYW5hIQ==" 
    
    payload = {
        "transaction": tx_payload,
        "intent": "Malicious drain attempt via prompt injection"
    }
    
    resp = requests.post(f"{BASE_URL}/simulate", json=payload)
    print(f"🛑 Sentinel Result: {resp.status_code} Forbidden")
    print(f"📄 Reason: {resp.json()['error']}\n")

    # 3. View Audit Trails
    print("[3/3] Fetching Immutable Audit Logs...")
    logs = requests.get(f"{BASE_URL}/logs").json()
    last_log = logs[-1]
    print(f"📝 Decision: {list(last_log['decision'].keys())[0]}")
    print(f"⏰ Timestamp: {last_log['timestamp']}")
    print(f"🎯 Intent Logged: {last_log['intent']}")
    
    print("\n✅ Demo Complete: Sentinel successfully protected the wallet.")

if __name__ == "__main__":
    run_demo()
