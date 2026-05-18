use crate::risk::oracle::{RiskOracle, RiskOracleError, RiskScore};
use crate::transaction::Address;

/// Webacy risk oracle client.
///
/// Queries the Webacy API for address risk scores.
/// This is the first concrete RiskOracle implementation.
pub struct WebacyClient {
    #[allow(dead_code)]
    api_key: String,
    base_url: String,
}

impl WebacyClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.webacy.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

#[async_trait::async_trait]
impl RiskOracle for WebacyClient {
    async fn score(&self, _address: &Address) -> Result<RiskScore, RiskOracleError> {
        // Placeholder: returns a safe default until Webacy API integration.
        // TODO: implement actual HTTP call to Webacy risk endpoint.
        Ok(RiskScore::new(0))
    }

    fn provider_name(&self) -> &str {
        "Webacy"
    }
}
