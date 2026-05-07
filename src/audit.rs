use crate::simulation::SimulationResult;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sled::Db;
use solana_sdk::hash::hash;
use solana_sdk::transaction::Transaction;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decision {
    Allowed,
    Blocked(String),
    PendingApproval(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum AuditResult {
    Allowed,
    #[default]
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransactionDetails {
    pub request_payload_base64: Option<String>,
    pub signature: Option<String>,
    #[serde(default)]
    pub program_ids: Vec<String>,
    #[serde(default)]
    pub account_keys: Vec<String>,
}

impl TransactionDetails {
    pub fn from_request_payload(request_payload_base64: String) -> Self {
        Self {
            request_payload_base64: Some(request_payload_base64),
            signature: None,
            program_ids: vec![],
            account_keys: vec![],
        }
    }

    pub fn from_transaction_request(request_payload_base64: String, tx: &Transaction) -> Self {
        Self {
            request_payload_base64: Some(request_payload_base64),
            signature: tx.signatures.first().map(|sig| sig.to_string()),
            program_ids: tx
                .message
                .instructions
                .iter()
                .filter_map(|ix| {
                    tx.message
                        .account_keys
                        .get(usize::from(ix.program_id_index))
                        .map(|key| key.to_string())
                })
                .collect(),
            account_keys: tx
                .message
                .account_keys
                .iter()
                .map(|k| k.to_string())
                .collect(),
        }
    }

    pub fn from_transaction(tx: &Transaction) -> Self {
        Self {
            request_payload_base64: None,
            signature: tx.signatures.first().map(|sig| sig.to_string()),
            program_ids: tx
                .message
                .instructions
                .iter()
                .filter_map(|ix| {
                    tx.message
                        .account_keys
                        .get(usize::from(ix.program_id_index))
                        .map(|key| key.to_string())
                })
                .collect(),
            account_keys: tx
                .message
                .account_keys
                .iter()
                .map(|k| k.to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: u64,
    #[serde(default)]
    pub transaction_id: Option<String>,
    pub transaction_signature: Option<String>,
    pub decision: Decision,
    pub simulation_result: Option<SimulationResult>,
    pub intent: Option<String>,
    #[serde(default)]
    pub result: AuditResult,
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub simulation_logs: Vec<String>,
    #[serde(default)]
    pub transaction_details: Option<TransactionDetails>,
}

pub fn hash_transaction_payload(payload: &str) -> String {
    hash(payload.as_bytes()).to_string()
}

pub struct AuditLogger {
    db: Db,
}

impl AuditLogger {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path).map_err(|e| anyhow!("Failed to open sled database: {}", e))?;
        Ok(Self { db })
    }

    pub fn log(&self, entry: AuditEntry) -> Result<()> {
        let key = self
            .db
            .generate_id()
            .map_err(|e| anyhow!("Failed to generate sled id: {}", e))?
            .to_be_bytes();
        let value = serde_json::to_vec(&entry)
            .map_err(|e| anyhow!("Failed to serialize audit entry: {}", e))?;

        self.db
            .insert(key, value)
            .map_err(|e| anyhow!("Failed to insert into sled: {}", e))?;

        self.db
            .flush()
            .map_err(|e| anyhow!("Failed to flush sled: {}", e))?;

        Ok(())
    }

    pub fn get_logs(&self) -> Result<Vec<AuditEntry>> {
        let mut logs = Vec::new();
        for item in self.db.iter() {
            let (_key, value) = item.map_err(|e| anyhow!("Sled iteration error: {}", e))?;
            let entry: AuditEntry = serde_json::from_slice(&value)
                .map_err(|e| anyhow!("Failed to deserialize audit entry: {}", e))?;
            logs.push(entry);
        }
        Ok(logs)
    }

    pub fn get_logs_filtered(
        &self,
        transaction_id: Option<&str>,
        signature: Option<&str>,
        result: Option<AuditResult>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AuditEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut logs = Vec::new();
        for item in self.db.iter() {
            let (_key, value) = item.map_err(|e| anyhow!("Sled iteration error: {}", e))?;
            let entry: AuditEntry = serde_json::from_slice(&value)
                .map_err(|e| anyhow!("Failed to deserialize audit entry: {}", e))?;

            if let Some(transaction_id) = transaction_id {
                if entry.transaction_id.as_deref() != Some(transaction_id) {
                    continue;
                }
            }

            if let Some(signature) = signature {
                if entry.transaction_signature.as_deref() != Some(signature) {
                    continue;
                }
            }

            if let Some(result) = result {
                if entry.result != result {
                    continue;
                }
            }

            logs.push(entry);
        }

        logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(logs.into_iter().skip(offset).take(limit).collect())
    }

    pub fn get_logs_by_transaction_id(&self, transaction_id: &str) -> Result<Vec<AuditEntry>> {
        self.get_logs_filtered(Some(transaction_id), None, None, 0, usize::MAX)
    }

    pub fn get_logs_by_signature(&self, signature: &str) -> Result<Vec<AuditEntry>> {
        self.get_logs_filtered(None, Some(signature), None, 0, usize::MAX)
    }

    /// Return total count of entries (optionally filtered by result).
    pub fn count(&self, filter_result: Option<AuditResult>) -> Result<AuditStats> {
        let mut total: u64 = 0;
        let mut allowed: u64 = 0;
        let mut blocked: u64 = 0;

        for item in self.db.iter() {
            let (_key, value) = item.map_err(|e| anyhow!("Sled iteration error: {}", e))?;
            let entry: AuditEntry = serde_json::from_slice(&value)
                .map_err(|e| anyhow!("Failed to deserialize audit entry: {}", e))?;

            match entry.result {
                AuditResult::Allowed => allowed += 1,
                AuditResult::Blocked => blocked += 1,
            }
            total += 1;
        }

        if let Some(filter) = filter_result {
            match filter {
                AuditResult::Allowed => Ok(AuditStats {
                    total: allowed,
                    allowed,
                    blocked: 0,
                }),
                AuditResult::Blocked => Ok(AuditStats {
                    total: blocked,
                    allowed: 0,
                    blocked,
                }),
            }
        } else {
            Ok(AuditStats {
                total,
                allowed,
                blocked,
            })
        }
    }

    /// Count entries matching the given filter criteria (for pagination metadata).
    pub fn count_filtered(
        &self,
        transaction_id: Option<&str>,
        signature: Option<&str>,
        result: Option<AuditResult>,
    ) -> Result<usize> {
        let mut count = 0usize;
        for item in self.db.iter() {
            let (_key, value) = item.map_err(|e| anyhow!("Sled iteration error: {}", e))?;
            let entry: AuditEntry = serde_json::from_slice(&value)
                .map_err(|e| anyhow!("Failed to deserialize audit entry: {}", e))?;

            if let Some(tid) = transaction_id {
                if entry.transaction_id.as_deref() != Some(tid) {
                    continue;
                }
            }
            if let Some(sig) = signature {
                if entry.transaction_signature.as_deref() != Some(sig) {
                    continue;
                }
            }
            if let Some(r) = result {
                if entry.result != r {
                    continue;
                }
            }
            count += 1;
        }
        Ok(count)
    }

    /// Delete a single audit entry by its sled key id.
    pub fn delete_by_id(&self, id: u64) -> Result<bool> {
        let key = id.to_be_bytes();
        let removed = self
            .db
            .remove(key)
            .map_err(|e| anyhow!("Failed to remove from sled: {}", e))?;
        self.db
            .flush()
            .map_err(|e| anyhow!("Failed to flush sled: {}", e))?;
        Ok(removed.is_some())
    }

    /// Clear all audit entries. Returns the number of entries removed.
    pub fn clear(&self) -> Result<u64> {
        let mut count = 0u64;
        for item in self.db.iter() {
            let (key, _value) = item.map_err(|e| anyhow!("Sled iteration error: {}", e))?;
            self.db
                .remove(key)
                .map_err(|e| anyhow!("Failed to remove from sled: {}", e))?;
            count += 1;
        }
        self.db
            .flush()
            .map_err(|e| anyhow!("Failed to flush sled: {}", e))?;
        Ok(count)
    }

    /// Check if the database is healthy (can be read).
    pub fn is_healthy(&self) -> bool {
        self.db.iter().next().is_some() || self.db.is_empty()
    }

    /// Return the underlying sled DB size on disk in bytes.
    pub fn size_on_disk(&self) -> u64 {
        self.db.size_on_disk().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total: u64,
    pub allowed: u64,
    pub blocked: u64,
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{AuditEntry, AuditLogger, AuditResult, Decision};
    use tempfile::TempDir;

    fn make_entry(
        timestamp: u64,
        transaction_id: Option<&str>,
        signature: Option<&str>,
        result: AuditResult,
    ) -> AuditEntry {
        let decision = match result {
            AuditResult::Allowed => Decision::Allowed,
            AuditResult::Blocked => Decision::Blocked("blocked".to_string()),
        };

        AuditEntry {
            timestamp,
            transaction_id: transaction_id.map(str::to_string),
            transaction_signature: signature.map(str::to_string),
            decision,
            simulation_result: None,
            intent: None,
            result,
            reasoning: String::new(),
            simulation_logs: vec![],
            transaction_details: None,
        }
    }

    #[test]
    fn filtered_queries_return_expected_logs() {
        let temp_dir = TempDir::new().expect("temp dir");
        let db_path = temp_dir.path().join("audit.sled");
        let logger = AuditLogger::new(db_path).expect("logger");

        logger
            .log(make_entry(1, Some("tx-a"), Some("sig-a"), AuditResult::Allowed))
            .expect("log");
        logger
            .log(make_entry(2, Some("tx-b"), Some("sig-b"), AuditResult::Blocked))
            .expect("log");
        logger
            .log(make_entry(3, Some("tx-c"), Some("sig-c"), AuditResult::Allowed))
            .expect("log");

        let by_tx = logger
            .get_logs_by_transaction_id("tx-b")
            .expect("query by tx id");
        assert_eq!(by_tx.len(), 1);
        assert_eq!(by_tx[0].transaction_id.as_deref(), Some("tx-b"));

        let by_sig = logger
            .get_logs_by_signature("sig-c")
            .expect("query by signature");
        assert_eq!(by_sig.len(), 1);
        assert_eq!(by_sig[0].transaction_signature.as_deref(), Some("sig-c"));

        let allowed = logger
            .get_logs_filtered(None, None, Some(AuditResult::Allowed), 0, 10)
            .expect("query allowed");
        assert_eq!(allowed.len(), 2);
        assert!(allowed.iter().all(|entry| entry.result == AuditResult::Allowed));
        assert!(allowed[0].timestamp >= allowed[1].timestamp);

        let paginated = logger
            .get_logs_filtered(None, None, None, 1, 1)
            .expect("paginated query");
        assert_eq!(paginated.len(), 1);
        assert_eq!(paginated[0].timestamp, 2);
    }
}
