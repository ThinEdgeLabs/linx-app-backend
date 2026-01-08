use std::env;

#[derive(Clone, Debug)]
pub enum Network {
    Devnet,
    Testnet,
    Mainnet,
    Custom(String, NetworkType),
}

#[derive(Clone, Debug)]
pub enum NetworkType {
    Devnet,
    Testnet,
    Mainnet,
}

impl Network {
    /// Returns the base URL for the network.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the network instance.
    ///
    /// # Returns
    ///
    /// A string containing the base URL of the network.
    pub fn base_url(&self) -> String {
        match self {
            Network::Devnet => {
                env::var("DEV_NODE_URL").unwrap_or_else(|_| "http://127.0.0.1:12973".to_owned())
            }
            Network::Testnet => env::var("TESTNET_NODE_URL")
                .unwrap_or_else(|_| "https://node.testnet.alephium.org".to_owned()),
            Network::Mainnet => env::var("MAINNET_NODE_URL")
                .unwrap_or_else(|_| "https://node.mainnet.alephium.org".to_owned()),
            Network::Custom(url, _) => url.clone(),
        }
    }

    pub fn identifier(&self) -> String {
        match self {
            Network::Devnet => "devnet".to_string(),
            Network::Testnet => "testnet".to_string(),
            Network::Mainnet => "mainnet".to_string(),
            Network::Custom(_, network_type) => network_type.to_string(),
        }
    }

    /// Creates a custom network with the specified URL and network type
    pub fn custom(url: &str, network_type: NetworkType) -> Self {
        Network::Custom(url.to_string(), network_type)
    }
}

impl std::fmt::Display for NetworkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                NetworkType::Devnet => "devnet",
                NetworkType::Testnet => "testnet",
                NetworkType::Mainnet => "mainnet",
            }
        )
    }
}

impl Default for Network {
    fn default() -> Self {
        env::var("NETWORK")
            .map(|env| match env.as_str() {
                "devnet" => Network::Devnet,
                "testnet" => Network::Testnet,
                "mainnet" => Network::Mainnet,
                _ => Network::Mainnet,
            })
            .unwrap_or(Network::Mainnet)
    }
}

impl From<Network> for String {
    fn from(value: Network) -> Self {
        match value {
            Network::Devnet => "devnet".to_string(),
            Network::Testnet => "testnet".to_string(),
            Network::Mainnet => "mainnet".to_string(),
            Network::Custom(_, network_type) => network_type.to_string(),
        }
    }
}

impl From<String> for Network {
    fn from(value: String) -> Self {
        match value.as_str() {
            "devnet" => Network::Devnet,
            "testnet" => Network::Testnet,
            "mainnet" => Network::Mainnet,
            _ => panic!("Invalid network type"),
        }
    }
}

impl From<String> for NetworkType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "devnet" => NetworkType::Devnet,
            "testnet" => NetworkType::Testnet,
            "mainnet" => NetworkType::Mainnet,
            _ => panic!("Invalid network type"),
        }
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use std::env;

    fn cleanup_env_vars() {
        env::remove_var("DEV_NODE_URL");
        env::remove_var("TESTNET_NODE_URL");
        env::remove_var("MAINNET_NODE_URL");
        env::remove_var("NETWORK");
    }

    #[test]
    fn test_base_url_with_env_vars() {
        cleanup_env_vars();

        // Set environment variables
        env::set_var("DEV_NODE_URL", "http://custom-dev.example.com");
        env::set_var("TESTNET_NODE_URL", "http://custom-testnet.example.com");
        env::set_var("MAINNET_NODE_URL", "http://custom-mainnet.example.com");

        // Test that URLs come from environment variables when set
        assert_eq!(Network::Devnet.base_url(), "http://custom-dev.example.com");
        assert_eq!(Network::Testnet.base_url(), "http://custom-testnet.example.com");
        assert_eq!(Network::Mainnet.base_url(), "http://custom-mainnet.example.com");

        cleanup_env_vars();
    }

    #[test]
    fn test_identifier() {
        assert_eq!(Network::Devnet.identifier(), "devnet");
        assert_eq!(Network::Testnet.identifier(), "testnet");
        assert_eq!(Network::Mainnet.identifier(), "mainnet");

        let custom_network = Network::Custom("http://example.com".to_string(), NetworkType::Devnet);
        assert_eq!(custom_network.identifier(), "devnet");
    }

    #[test]
    fn test_custom_network_creation() {
        let url = "http://custom.example.com";
        let network_type = NetworkType::Testnet;

        let custom_network = Network::custom(url, network_type);

        // Verify it's created correctly
        match custom_network {
            Network::Custom(stored_url, stored_type) => {
                assert_eq!(stored_url, url);
                match stored_type {
                    NetworkType::Testnet => (),
                    _ => panic!("Wrong network type stored"),
                }
            }
            _ => panic!("Should have created a Custom network variant"),
        }
    }

    #[test]
    fn test_network_type_to_string() {
        assert_eq!(NetworkType::Devnet.to_string(), "devnet");
        assert_eq!(NetworkType::Testnet.to_string(), "testnet");
        assert_eq!(NetworkType::Mainnet.to_string(), "mainnet");
    }

    #[test]
    fn test_network_default() {
        cleanup_env_vars();

        // Test default when NETWORK is not set
        let default_network = Network::default();
        match default_network {
            Network::Mainnet => (),
            _ => panic!("Default should be Mainnet when NETWORK is not set"),
        }

        // Test with NETWORK set to devnet
        env::set_var("NETWORK", "devnet");
        let dev_network = Network::default();
        match dev_network {
            Network::Devnet => (),
            _ => panic!("Should be Devnet when NETWORK is set to devnet"),
        }

        // Test with NETWORK set to testnet
        env::set_var("NETWORK", "testnet");
        let test_network = Network::default();
        match test_network {
            Network::Testnet => (),
            _ => panic!("Should be Testnet when NETWORK is set to testnet"),
        }

        // Test with NETWORK set to mainnet
        env::set_var("NETWORK", "mainnet");
        let main_network = Network::default();
        match main_network {
            Network::Mainnet => (),
            _ => panic!("Should be Mainnet when NETWORK is set to mainnet"),
        }

        // Test with NETWORK set to something else
        env::set_var("NETWORK", "unknown");
        let unknown_network = Network::default();
        match unknown_network {
            Network::Mainnet => (),
            _ => panic!("Should default to Mainnet for unknown network"),
        }

        cleanup_env_vars();
    }

    #[test]
    fn test_network_to_string_conversion() {
        let devnet_string: String = Network::Devnet.into();
        assert_eq!(devnet_string, "devnet");

        let testnet_string: String = Network::Testnet.into();
        assert_eq!(testnet_string, "testnet");

        let mainnet_string: String = Network::Mainnet.into();
        assert_eq!(mainnet_string, "mainnet");

        let custom_network =
            Network::Custom("http://example.com".to_string(), NetworkType::Testnet);
        let custom_string: String = custom_network.into();
        assert_eq!(custom_string, "testnet");
    }

    #[test]
    fn test_string_to_network_conversion() {
        let devnet: Network = "devnet".to_string().into();
        match devnet {
            Network::Devnet => (),
            _ => panic!("Should convert 'devnet' string to Network::Devnet"),
        }

        let testnet: Network = "testnet".to_string().into();
        match testnet {
            Network::Testnet => (),
            _ => panic!("Should convert 'testnet' string to Network::Testnet"),
        }

        let mainnet: Network = "mainnet".to_string().into();
        match mainnet {
            Network::Mainnet => (),
            _ => panic!("Should convert 'mainnet' string to Network::Mainnet"),
        }
    }

    #[test]
    #[should_panic(expected = "Invalid network type")]
    fn test_invalid_string_to_network_conversion() {
        let _invalid: Network = "invalid".to_string().into();
        // This should panic
    }

    #[test]
    fn test_string_to_network_type_conversion() {
        let devnet: NetworkType = "devnet".to_string().into();
        match devnet {
            NetworkType::Devnet => (),
            _ => panic!("Should convert 'devnet' string to NetworkType::Devnet"),
        }

        let testnet: NetworkType = "testnet".to_string().into();
        match testnet {
            NetworkType::Testnet => (),
            _ => panic!("Should convert 'testnet' string to NetworkType::Testnet"),
        }

        let mainnet: NetworkType = "mainnet".to_string().into();
        match mainnet {
            NetworkType::Mainnet => (),
            _ => panic!("Should convert 'mainnet' string to NetworkType::Mainnet"),
        }
    }

    #[test]
    #[should_panic(expected = "Invalid network type")]
    fn test_invalid_string_to_network_type_conversion() {
        let _invalid: NetworkType = "invalid".to_string().into();
        // This should panic
    }

    #[test]
    fn test_clone() {
        let network = Network::Testnet;
        let cloned_network = network.clone();

        match cloned_network {
            Network::Testnet => (),
            _ => panic!("Cloned network should match original"),
        }

        let network_type = NetworkType::Devnet;
        let cloned_type = network_type.clone();

        match cloned_type {
            NetworkType::Devnet => (),
            _ => panic!("Cloned network type should match original"),
        }
    }

    #[test]
    fn test_debug() {
        // This test simply ensures that Debug is implemented correctly
        let network = Network::Mainnet;
        let debug_output = format!("{:?}", network);
        assert!(debug_output.contains("Mainnet"));

        let network_type = NetworkType::Testnet;
        let debug_output = format!("{:?}", network_type);
        assert!(debug_output.contains("Testnet"));
    }
}
