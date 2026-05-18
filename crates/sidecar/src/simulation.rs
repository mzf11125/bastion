use anyhow::{Result, anyhow};
use base64::Engine as _;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_sdk::transaction::Transaction;
use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub logs: Vec<String>,
    pub units_consumed: Option<u64>,
    pub return_data: Option<ReturnData>,
    pub error: Option<serde_json::Value>,
    #[serde(default)]
    pub balance_changes: HashMap<String, i64>,
    #[serde(default)]
    pub simulation_hash: Option<[u8; 32]>,
}

pub fn compute_simulation_hash(logs: &[String], error: &Option<serde_json::Value>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for log in logs {
        hasher.update(log.as_bytes());
        hasher.update(b"\0");
    }
    if let Some(err) = error {
        hasher.update(err.to_string().as_bytes());
    }
    let hash = hasher.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

pub trait Simulate: Send + Sync {
    fn simulate_transaction(&self, tx: &Transaction) -> Result<SimulationResult>;
}

pub struct HeliusSimulator {
    api_key: String,
    client: Client,
    rpc_url: String,
}

impl HeliusSimulator {
    pub fn new() -> Result<Self> {
        let api_key = env::var("HELIUS_API_KEY")
            .map_err(|err| anyhow!("HELIUS_API_KEY must be set in environment: {err}"))?;
        Ok(Self {
            api_key,
            client: Client::new(),
            rpc_url: "https://mainnet.helius-rpc.com/".to_string(),
        })
    }

    pub fn with_rpc_url(rpc_url: impl Into<String>) -> Result<Self> {
        let api_key = env::var("HELIUS_API_KEY")
            .map_err(|err| anyhow!("HELIUS_API_KEY must be set in environment: {err}"))?;
        Ok(Self {
            api_key,
            client: Client::new(),
            rpc_url: rpc_url.into(),
        })
    }

    fn rpc_endpoint(&self) -> String {
        format!("{}?api-key={}", self.rpc_url, self.api_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnData {
    pub data: String,
    pub encoding: String,
    pub program_id: String,
}

#[derive(Deserialize, Debug)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize, Debug)]
struct RpcResult {
    value: RpcValue,
}

#[derive(Deserialize, Debug)]
struct RpcValue {
    logs: Option<Vec<String>>,
    #[serde(rename = "unitsConsumed")]
    units_consumed: Option<u64>,
    #[serde(rename = "returnData")]
    return_data: Option<RpcReturnData>,
    err: Option<serde_json::Value>,
    #[serde(rename = "accounts")]
    accounts: Option<Vec<Option<RpcAccount>>>,
}

#[derive(Deserialize, Debug)]
struct RpcAccount {
    lamports: u64,
}

#[derive(Deserialize, Debug)]
struct RpcError {
    message: String,
}

#[derive(Deserialize, Debug)]
struct RpcReturnData {
    data: Vec<String>,
    #[serde(rename = "programId")]
    program_id: String,
}

impl Simulate for HeliusSimulator {
    fn simulate_transaction(&self, tx: &Transaction) -> Result<SimulationResult> {
        let serialized_tx = bincode::serialize(tx)
            .map_err(|err| anyhow!("Failed to serialize transaction: {err}"))?;
        let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized_tx);

        let accounts_to_track: Vec<String> = tx
            .message
            .account_keys
            .iter()
            .map(|k| k.to_string())
            .collect();

        let pre_balances = self.fetch_pre_balances(&accounts_to_track)?;

        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": [
                base64_tx,
                {
                    "encoding": "base64",
                    "sigVerify": false,
                    "replaceRecentBlockhash": true,
                    "accounts": {
                        "encoding": "base64",
                        "addresses": accounts_to_track
                    }
                }
            ]
        });

        let response = self
            .client
            .post(self.rpc_endpoint())
            .json(&request_body)
            .send()
            .map_err(|err| anyhow!("Failed to send simulation request to Helius: {err}"))?
            .error_for_status()
            .map_err(|err| anyhow!("Helius simulateTransaction HTTP error: {err}"))?;

        let rpc_resp: RpcResponse<RpcResult> = response
            .json()
            .map_err(|err| anyhow!("Failed to parse Helius response JSON: {err}"))?;

        if let Some(err) = rpc_resp.error {
            return Err(anyhow!("RPC Error: {}", err.message));
        }

        let result = rpc_resp
            .result
            .ok_or_else(|| anyhow!("No result in RPC response"))?
            .value;

        let return_data = match result.return_data {
            Some(data) => {
                let mut iter = data.data.into_iter();
                let payload = iter
                    .next()
                    .ok_or_else(|| anyhow!("returnData missing payload"))?;
                let encoding = iter
                    .next()
                    .ok_or_else(|| anyhow!("returnData missing encoding"))?;
                Some(ReturnData {
                    data: payload,
                    encoding,
                    program_id: data.program_id,
                })
            }
            None => None,
        };

        let mut balance_changes = HashMap::new();
        if let Some(accounts) = result.accounts {
            for (i, account_opt) in accounts.into_iter().enumerate() {
                let address = tx.message.account_keys[i].to_string();
                let pre = pre_balances.get(&address).copied().unwrap_or(0);
                let post = account_opt.map(|a| a.lamports as i64).unwrap_or(0);
                let delta = post - pre;
                if delta != 0 {
                    balance_changes.insert(address, delta);
                }
            }
        }

        let sim_logs = result.logs.unwrap_or_default();
        let sim_hash = compute_simulation_hash(&sim_logs, &result.err);

        Ok(SimulationResult {
            logs: sim_logs,
            units_consumed: result.units_consumed,
            return_data,
            error: result.err,
            balance_changes,
            simulation_hash: Some(sim_hash),
        })
    }
}

impl HeliusSimulator {
    fn fetch_pre_balances(&self, addresses: &[String]) -> Result<HashMap<String, i64>> {
        if addresses.is_empty() {
            return Ok(HashMap::new());
        }

        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMultipleAccounts",
            "params": [
                addresses,
                { "encoding": "base64" }
            ]
        });

        #[derive(Deserialize)]
        struct AccountInfo {
            lamports: u64,
        }

        #[derive(Deserialize)]
        struct MultiResult {
            value: Vec<Option<AccountInfo>>,
        }

        let response = self
            .client
            .post(self.rpc_endpoint())
            .json(&request_body)
            .send()
            .map_err(|err| anyhow!("Failed to fetch pre-balances: {err}"))?
            .error_for_status()
            .map_err(|err| anyhow!("Pre-balance HTTP error: {err}"))?;

        let rpc_resp: RpcResponse<MultiResult> = response
            .json()
            .map_err(|err| anyhow!("Failed to parse pre-balance response: {err}"))?;

        if let Some(err) = rpc_resp.error {
            return Err(anyhow!("Pre-balance RPC Error: {}", err.message));
        }

        let mut balances = HashMap::new();
        if let Some(result) = rpc_resp.result {
            for (i, account) in result.value.into_iter().enumerate() {
                if let Some(Some(account)) = Some(account)
                    && i < addresses.len()
                {
                    balances.insert(addresses[i].clone(), account.lamports as i64);
                }
            }
        }
        Ok(balances)
    }
}
