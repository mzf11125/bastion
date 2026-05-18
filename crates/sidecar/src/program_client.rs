use anyhow::{Result, anyhow};
use base64::Engine as _;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_sdk::{
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::str::FromStr;
use std::sync::Arc;

const BASTION_PROGRAM_ID: &str = "BaSZuLcwjfh75T3TjbVYpTH4qpJt1tNoZ3S6PTkvNhCb";
const AUDIT_SEED: &[u8] = b"bastion_audit";

#[derive(Debug, Clone, Serialize)]
pub struct OnChainAuditResult {
    pub signature: String,
    pub slot: u64,
}

#[derive(Clone)]
pub struct OnChainClient {
    authority: Arc<Keypair>,
    program_id: Pubkey,
    rpc_url: String,
    http: Client,
    enabled: bool,
}

impl OnChainClient {
    pub fn new(rpc_url: String, keypair_path: String, enabled: bool) -> Result<Self> {
        let authority = if keypair_path.is_empty() {
            return Err(anyhow!("Keypair path is empty"));
        } else {
            let bytes = std::fs::read(&keypair_path)
                .map_err(|e| anyhow!("Failed to read keypair file {keypair_path}: {e}"))?;
            Keypair::from_bytes(&bytes).map_err(|e| anyhow!("Failed to parse keypair: {e}"))?
        };

        Ok(Self {
            authority: Arc::new(authority),
            program_id: Pubkey::from_str(BASTION_PROGRAM_ID).expect("Invalid BASTION_PROGRAM_ID"),
            rpc_url,
            http: Client::new(),
            enabled,
        })
    }

    pub fn disabled() -> Self {
        Self {
            authority: Arc::new(Keypair::new()),
            program_id: Pubkey::from_str(BASTION_PROGRAM_ID).expect("Invalid BASTION_PROGRAM_ID"),
            rpc_url: "http://localhost:8899".to_string(),
            http: Client::new(),
            enabled: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn find_audit_state_address(&self) -> Pubkey {
        Pubkey::find_program_address(&[AUDIT_SEED], &self.program_id).0
    }

    fn find_audit_entry_address(&self, total_audits: u64) -> Pubkey {
        Pubkey::find_program_address(&[AUDIT_SEED, &total_audits.to_le_bytes()], &self.program_id).0
    }

    pub async fn log_audit(
        &self,
        decision: u8,
        simulation_result: [u8; 32],
        reasoning: &str,
        program_id_opt: Option<[u8; 32]>,
    ) -> Result<OnChainAuditResult> {
        if !self.enabled {
            return Err(anyhow!("On-chain client is disabled"));
        }

        let audit_state = self.find_audit_state_address();
        let signer = self.authority.pubkey();
        let system_program = solana_sdk::system_program::ID;

        let total_audits = self.get_audit_state_total(&audit_state).await.unwrap_or(0);

        let audit_entry = self.find_audit_entry_address(total_audits);

        let mut data_args: Vec<u8> = Vec::new();
        data_args.push(decision);
        data_args.extend_from_slice(&simulation_result);
        let reasoning_bytes = reasoning.as_bytes();
        data_args.extend_from_slice(&(reasoning_bytes.len() as u32).to_le_bytes());
        data_args.extend_from_slice(reasoning_bytes);
        match program_id_opt {
            Some(pid) => {
                data_args.push(1);
                data_args.extend_from_slice(&pid);
            }
            None => {
                data_args.push(0);
            }
        }

        let data = serialize_anchor_instruction("logAudit", &data_args)?;

        let accounts = vec![
            AccountMeta::new(audit_entry, false),
            AccountMeta::new(audit_state, false),
            AccountMeta::new(signer, true),
            AccountMeta::new_readonly(system_program, false),
        ];

        let ix = Instruction {
            program_id: self.program_id,
            accounts,
            data,
        };

        self.send_and_confirm(ix).await
    }

    pub async fn emergency_pause(&self) -> Result<OnChainAuditResult> {
        if !self.enabled {
            return Err(anyhow!("On-chain client is disabled"));
        }

        let audit_state = self.find_audit_state_address();
        let signer = self.authority.pubkey();

        let data = serialize_anchor_instruction("emergencyPause", &[])?;

        let accounts = vec![
            AccountMeta::new(audit_state, false),
            AccountMeta::new_readonly(signer, true),
        ];

        let ix = Instruction {
            program_id: self.program_id,
            accounts,
            data,
        };

        self.send_and_confirm(ix).await
    }

    pub async fn emergency_resume(&self) -> Result<OnChainAuditResult> {
        if !self.enabled {
            return Err(anyhow!("On-chain client is disabled"));
        }

        let audit_state = self.find_audit_state_address();
        let signer = self.authority.pubkey();

        let data = serialize_anchor_instruction("emergencyResume", &[])?;

        let accounts = vec![
            AccountMeta::new(audit_state, false),
            AccountMeta::new_readonly(signer, true),
        ];

        let ix = Instruction {
            program_id: self.program_id,
            accounts,
            data,
        };

        self.send_and_confirm(ix).await
    }

    async fn get_audit_state_total(&self, audit_state: &Pubkey) -> Option<u64> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                audit_state.to_string(),
                { "encoding": "base64" }
            ]
        });

        let resp: RpcResponse<GetAccountInfoResult> = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        let value = resp.result?.value?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&value.data[0])
            .ok()?;

        if bytes.len() >= 16 {
            Some(u64::from_le_bytes(bytes[8..16].try_into().ok()?))
        } else {
            None
        }
    }

    async fn send_and_confirm(&self, ix: Instruction) -> Result<OnChainAuditResult> {
        let blockhash = self.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.authority.pubkey()),
            &[self.authority.as_ref()],
            blockhash,
        );

        let tx_bytes =
            bincode::serialize(&tx).map_err(|e| anyhow!("Failed to serialize tx: {e}"))?;
        let tx_b64 = base64::engine::general_purpose::STANDARD.encode(tx_bytes);

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                tx_b64,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                    "preflightCommitment": "confirmed"
                }
            ]
        });

        let resp: RpcResponse<String> = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to send transaction: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse response: {e}"))?;

        let sig = resp.result.ok_or_else(|| {
            anyhow!(
                "Transaction failed: {}",
                resp.error
                    .unwrap_or(RpcErrorMessage {
                        message: "unknown error".to_string()
                    })
                    .message
            )
        })?;

        self.confirm_transaction(&sig).await?;

        Ok(OnChainAuditResult {
            signature: sig,
            slot: 0,
        })
    }

    async fn get_latest_blockhash(&self) -> Result<Hash> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": []
        });

        let resp: RpcResponse<LatestBlockhashResult> = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to get blockhash: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse blockhash: {e}"))?;

        let result = resp.result.ok_or_else(|| anyhow!("No blockhash result"))?;

        Hash::from_str(&result.value.blockhash).map_err(|e| anyhow!("Invalid blockhash: {e}"))
    }

    async fn confirm_transaction(&self, signature: &str) -> Result<()> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[signature]]
        });

        for _ in 0..30 {
            let resp: RpcResponse<SignatureStatusesResult> = self
                .http
                .post(&self.rpc_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| anyhow!("Failed to check status: {e}"))?
                .json()
                .await
                .map_err(|e| anyhow!("Failed to parse status: {e}"))?;

            if let Some(result) = resp.result
                && let Some(Some(status)) = result.value.first()
                && (status.confirmation_status == "confirmed"
                    || status.confirmation_status == "finalized")
            {
                return Ok(());
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        Err(anyhow!("Transaction {signature} not confirmed after 15s"))
    }
}

fn sighash(namespace: &str, name: &str) -> [u8; 8] {
    let preimage = format!("{namespace}:{name}");
    let mut hasher = Sha256::new();
    hasher.update(preimage.as_bytes());
    let hash = hasher.finalize();
    let mut result = [0u8; 8];
    result.copy_from_slice(&hash[..8]);
    result
}

fn serialize_anchor_instruction(name: &str, args: &[u8]) -> Result<Vec<u8>> {
    let sighash = sighash("global", name);
    let mut data = sighash.to_vec();
    data.extend_from_slice(args);
    Ok(data)
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcErrorMessage>,
}

#[derive(Deserialize)]
struct RpcErrorMessage {
    message: String,
}

impl std::fmt::Display for RpcErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Deserialize)]
struct LatestBlockhashResult {
    value: LatestBlockhashValue,
}

#[derive(Deserialize)]
struct LatestBlockhashValue {
    blockhash: String,
}

#[derive(Deserialize)]
struct GetAccountInfoResult {
    value: Option<AccountInfoResult>,
}

#[derive(Deserialize)]
struct AccountInfoResult {
    data: Vec<String>,
}

#[derive(Deserialize)]
struct SignatureStatusesResult {
    value: Vec<Option<SignatureStatus>>,
}

#[derive(Deserialize)]
struct SignatureStatus {
    #[serde(rename = "confirmationStatus")]
    confirmation_status: String,
}
