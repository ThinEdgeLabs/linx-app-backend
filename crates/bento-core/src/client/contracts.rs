use anyhow::Result;
use async_trait::async_trait;
use bento_trait::stage::ContractsProvider;
use bento_types::{CallContractParams, CallContractResult};
use url::Url;

use super::Client;

#[async_trait]
impl ContractsProvider for Client {
    async fn call_contract(&self, params: CallContractParams) -> Result<CallContractResult> {
        let endpoint = "contracts/call-contract";
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let json_body = serde_json::to_string(&params)?;
        let response = self
            .inner
            .post(url)
            .header("Content-Type", "application/json")
            .body(json_body)
            .send()
            .await?;
        let response = response.error_for_status()?;

        let result: CallContractResult = response.json().await?;
        Ok(result)
    }
}
