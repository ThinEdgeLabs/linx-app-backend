use anyhow::Result;
use async_trait::async_trait;
use bento_trait::stage::TransactionProvider;
use bento_types::Transaction;
use serde::Deserialize;
use url::Url;

use super::Client;
#[async_trait]
impl TransactionProvider for Client {
    /// Get transaction details by transaction ID.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The ID of the transaction to retrieve.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Transaction` structure, or an error if the request fails.
    async fn get_tx_by_hash(&self, tx_id: &str) -> Result<Option<Transaction>> {
        let endpoint = format!("transactions/details/{}", tx_id);
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }

    /// List transactions of a block with pagination.
    /// GET:/blocks/{block_hash}/transactions?limit={limit}&offset={offset}
    async fn get_block_txs(
        &self,
        block_hash: String,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Transaction>> {
        let endpoint = format!("blocks/{block_hash}/transactions?limit={limit}&offset={offset}");
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;
        let response = self.inner.get(url).send().await?.json().await?;
        Ok(response)
    }
}

#[derive(Debug, Deserialize)]
pub struct SubmitTxResponse {
    #[serde(rename = "txId")]
    pub tx_id: String,
    #[serde(rename = "fromGroup")]
    pub from_group: i32,
    #[serde(rename = "toGroup")]
    pub to_group: i32,
}

impl Client {
    /// Submit a signed transaction to the Alephium network
    ///
    /// # Arguments
    /// * `unsigned_tx` - Unsigned transaction hex string
    /// * `signature` - Transaction signature hex string
    ///
    /// # Returns
    /// Transaction ID and routing groups on success
    pub async fn submit_transaction(
        &self,
        unsigned_tx: &str,
        signature: &str,
    ) -> Result<SubmitTxResponse> {
        let endpoint = "transactions/submit";
        let url = Url::parse(&format!("{}/{}", self.base_url, endpoint))?;

        let payload = serde_json::json!({
            "unsignedTx": unsigned_tx,
            "signature": signature
        });

        let body = serde_json::to_string(&payload)?;

        let response = self.inner
            .post(url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        // Check for HTTP errors and extract error details
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();

            // Try to extract "detail" field from JSON error response
            let error_message = if let Ok(json_error) = serde_json::from_str::<serde_json::Value>(&error_text) {
                json_error
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&error_text)
                    .to_string()
            } else {
                error_text
            };

            return Err(anyhow::anyhow!("{}", error_message));
        }

        let result: SubmitTxResponse = response.json().await?;
        Ok(result)
    }
}
