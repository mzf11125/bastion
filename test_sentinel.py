import requests
import base64
import json
import time

BASE_URL = "http://localhost:3000"

def test_hello():
    print("\n--- Testing Hello ---")
    resp = requests.get(f"{BASE_URL}/")
    print(f"Status: {resp.status_code}, Body: {resp.text}")

def test_simulate_blocked_program():
    print("\n--- Testing Blocked Program Policy ---")
    # A dummy transaction with a random program ID that is NOT in the allowlist
    # In config.toml allowed_programs = [] means ALL programs are allowed unless blocked_addresses is set?
    # Actually, looking at policy.rs, if allowed_programs is NOT empty, it acts as a whitelist.
    # Our config.toml had allowed_programs = [] which usually means "allow all" or "none" depending on impl.
    # Let's check policy.rs logic.
    
    # Payload: Base64 of a dummy unsigned tx
    # Using a fake but valid-looking base64
    tx_payload = "AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAQAGBwclS7KTAAAAAAA="
    
    payload = {
        "transaction": tx_payload,
        "intent": "Malicious drain attempt"
    }
    resp = requests.post(f"{BASE_URL}/simulate", json=payload)
    print(f"Status: {resp.status_code}")
    print(f"Response: {resp.text}")

def test_get_logs():
    print("\n--- Testing Audit Logs ---")
    resp = requests.get(f"{BASE_URL}/logs")
    if resp.status_code == 200:
        logs = resp.json()
        print(f"Retrieved {len(logs)} log entries")
        if logs:
            print(f"Latest entry decision: {logs[-1]['decision']}")
    else:
        print(f"Error: {resp.text}")

if __name__ == "__main__":
    try:
        test_hello()
        test_simulate_blocked_program()
        test_get_logs()
    except Exception as e:
        print(f"Failed to connect to Sentinel: {e}")
        print("Make sure the Sentinel server is running on port 3000.")
