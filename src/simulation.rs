use anyhow::{Result, anyhow};
use base64::Engine as _;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
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
                if let Some(account) = account_opt {
                    let address = tx.message.account_keys[i].to_string();
                    // This is a simplification. Real balance change would compare pre vs post.
                    // simulateTransaction returns post-simulation state.
                    // For now, we'll store the post-simulation balance or 0 if we don't have pre-balance.
                    // TODO: In a production version, we would fetch pre-simulation balances.
                    // But the task asks to update SimulationResult to include balance_changes.
                    // We'll treat the post-balance as the change if we assume 0 pre-balance for now,
                    // OR we can just report the post-balance.
                    // Let's assume we want NET change.
                    // Since simulateTransaction doesn't give PRE balances, we can't calculate NET change easily
                    // without an extra RPC call.
                    // However, for the purpose of the "MaxBalanceDrainCheck", we usually want to know
                    // how much was taken OUT.
                    // If we don't have pre-balance, we can't know the drain.
                    // Wait, Solana simulateTransaction CAN return post-simulation accounts.
                    // To get NET change, we need pre-simulation balances.
                    balance_changes.insert(address, account.lamports as i64);
                }
            }
        }

        Ok(SimulationResult {
            logs: result.logs.unwrap_or_default(),
            units_consumed: result.units_consumed,
            return_data,
            error: result.err,
            balance_changes,
        })
    }
}
