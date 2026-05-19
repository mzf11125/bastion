use bastion_core::{Address, RiskOracle, RiskOracleError, RiskScore};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct GrondRiskResponse {
    risk_score: u8,
    #[allow(dead_code)]
    confidence: f64,
    #[allow(dead_code)]
    summary: Option<String>,
}

#[derive(Clone)]
pub struct GrondOracle {
    api_url: String,
    client: reqwest::Client,
    cache: Arc<Mutex<HashMap<String, (Instant, RiskScore)>>>,
}

impl GrondOracle {
    pub fn new(api_url: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            api_url: api_url.into(),
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn disabled() -> Self {
        Self {
            api_url: String::new(),
            client: reqwest::Client::new(),
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn is_enabled(&self) -> bool {
        !self.api_url.is_empty()
    }
}

#[async_trait::async_trait]
impl RiskOracle for GrondOracle {
    async fn score(&self, address: &Address) -> Result<RiskScore, RiskOracleError> {
        let addr_str = address.to_string();

        {
            let cache = self.cache.lock().unwrap();
            if let Some((ts, score)) = cache.get(&addr_str)
                && ts.elapsed() < CACHE_TTL
            {
                return Ok(*score);
            }
        }

        let request_body = serde_json::json!({
            "target": addr_str,
            "chains": ["solana"],
            "analyst_id": "bastion-sidecar",
            "session_id": "grond-risk-check",
        });

        match self
            .client
            .post(format!("{}/api/v1/tools/address-risk", self.api_url))
            .json(&request_body)
            .timeout(Duration::from_secs(8))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<GrondRiskResponse>().await {
                Ok(data) => {
                    let score = RiskScore::new(data.risk_score);
                    let mut cache = self.cache.lock().unwrap();
                    cache.insert(addr_str, (Instant::now(), score));
                    Ok(score)
                }
                Err(_) => {
                    eprintln!(
                        "[grond] failed to parse risk response from {} for {}",
                        self.api_url, addr_str
                    );
                    Ok(RiskScore::new(0))
                }
            },
            Err(e) => {
                eprintln!(
                    "[grond] risk check failed for {} ({}): {}",
                    self.api_url, addr_str, e
                );
                Ok(RiskScore::new(0))
            }
        }
    }

    fn provider_name(&self) -> &str {
        "grond"
    }
}
