use std::time::Duration;

use bento_types::network::Network;
use reqwest::Client as ReqwestClient;
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
pub mod block;
pub mod contracts;
pub mod transaction;

pub use transaction::SubmitTxResponse;

/// Struct representing a client that interacts with the Alephium node network.
#[derive(Clone, Debug)]
pub struct Client {
    inner: reqwest_middleware::ClientWithMiddleware, // The inner HTTP client used for requests.
    pub network: Network,                            // The network the client is connected to.
    pub base_url: String, // The base URL for making requests to the node network.
}

impl Client {
    /// Creates a new `Client` instance for interacting with a specified network.
    ///
    /// # Arguments
    ///
    /// * `network` - The network to connect to.
    ///
    /// # Returns
    ///
    /// A new `Client` instance.
    pub fn new(network: Network) -> Self {
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(
                Duration::from_millis(100), // Minimum retry delay
                Duration::from_secs(1),     // Maximum retry delay
            )
            .build_with_max_retries(3);

        let client = ClientBuilder::new(ReqwestClient::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        Self { inner: client, network: network.clone(), base_url: network.base_url() }
    }
}
