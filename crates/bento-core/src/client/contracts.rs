use anyhow::Result;
use async_trait::async_trait;
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResult, ContractState};
use url::Url;

use super::Client;

#[async_trait]
impl ContractsProvider for Client {
    async fn call_contract(&self, params: CallContractParams) -> Result<CallContractResult> {
        let endpoint = "contracts/call-contract";
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let json_body = serde_json::to_string(&params)?;
        let response = self.inner.post(url).header("Content-Type", "application/json").body(json_body).send().await?;

        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            tracing::error!("call_contract failed (HTTP {}): {}", status, body);
            anyhow::bail!("call_contract HTTP {}: {}", status, body);
        }
        let result: CallContractResult = serde_json::from_str(&body)?;
        Ok(result)
    }
}

impl Client {
    pub async fn get_contract_state(&self, address: &str) -> Result<ContractState> {
        let url = Url::parse(&format!("{}/contracts/{}/state", self.base_url, address))?;
        let response = self.inner.get(url).send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            tracing::error!("get_contract_state failed (HTTP {}): {}", status, body);
            anyhow::bail!("get_contract_state HTTP {}: {}", status, body);
        }
        let result: ContractState = serde_json::from_str(&body)?;
        Ok(result)
    }
}
