use anyhow::Result;
use bento_cli::constants::{ALPH_TOKEN_ID, DUST_AMOUNT};
use bento_types::{CustomProcessorOutput, RichBlockEntry, Transaction};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::{fmt::Debug, str::FromStr};

use async_trait::async_trait;
use bento_core::ProcessorFactory;
use bento_core::db::DbPool;
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, processors::ProcessorOutput, utils::timestamp_millis_to_naive_datetime,
};
use bigdecimal::BigDecimal;

use crate::models::{NewAccountTransaction, NewTransferDetails, NewTransferTransactionDto};
use crate::processors::classifier::{TransactionCategory, TransactionClassifier};
use crate::repository::AccountTransactionRepository;

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, args: Option<serde_json::Value>| Box::new(TransferProcessor::new(db_pool, args))
}

pub struct TransferProcessor {
    connection_pool: Arc<DbPool>,
    gas_payer_addresses: HashSet<String>,
    repository: AccountTransactionRepository,
    classifier: TransactionClassifier,
}

impl TransferProcessor {
    pub fn new(connection_pool: Arc<DbPool>, args: Option<serde_json::Value>) -> Self {
        tracing::debug!("Initialized TransferProcessor");
        let gas_payer_addresses: HashSet<String> = args
            .and_then(|v| v.get("gas_payer_addresses").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        let repository = AccountTransactionRepository::new(connection_pool.clone());
        let classifier = TransactionClassifier::new(HashSet::new());
        Self { connection_pool, gas_payer_addresses, repository, classifier }
    }
}

impl Debug for TransferProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "TransferProcessor {{ connections: {:?}, idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[derive(Debug, Clone)]
pub struct TransferProcessorOutput {
    pub transfers: Vec<NewTransferTransactionDto>,
}

impl CustomProcessorOutput for TransferProcessorOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl ProcessorTrait for TransferProcessor {
    fn name(&self) -> &'static str {
        "transfer"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, bwe: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let all_transfers = bwe
            .iter()
            .flat_map(|el| {
                el.block
                    .transactions
                    .iter()
                    .filter(|tx| self.classifier.classify(tx) == TransactionCategory::Transfer)
                    .flat_map(|tx| {
                        extract_token_transfers(tx, &el.block, &self.gas_payer_addresses)
                    })
            })
            .collect();

        Ok(ProcessorOutput::Custom(Arc::new(TransferProcessorOutput { transfers: all_transfers })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            if let Some(transfer_output) = custom.as_any().downcast_ref::<TransferProcessorOutput>()
            {
                let transfers = &transfer_output.transfers;
                if !transfers.is_empty() {
                    self.repository.insert_transfers(transfers).await?;
                    tracing::info!("Inserted {} token transfers", transfers.len());
                }
            } else {
                return Err(anyhow::anyhow!("Invalid custom output type"));
            }
        } else {
            return Err(anyhow::anyhow!("Expected Custom output type"));
        }

        Ok(())
    }
}

fn extract_token_transfers(
    tx: &Transaction,
    block: &RichBlockEntry,
    gas_payer_addresses: &HashSet<String>,
) -> Vec<NewTransferTransactionDto> {
    let mut input_map: HashMap<(String, String), BigDecimal> = HashMap::new(); // (address, token_id) -> amount
    let mut output_map: HashMap<(String, String), BigDecimal> = HashMap::new();
    let mut input_addresses: HashSet<String> = HashSet::new();

    // Step 1: Parse inputs
    for input in &tx.unsigned.inputs {
        input_addresses.insert(input.address.clone());

        let alph_amount = input.atto_alph_amount.parse::<BigDecimal>().unwrap_or_default();
        *input_map.entry((input.address.clone(), ALPH_TOKEN_ID.to_string())).or_default() +=
            alph_amount;

        for token in &input.tokens {
            let amount = token.amount.parse::<BigDecimal>().unwrap_or_default();
            *input_map.entry((input.address.clone(), token.id.clone())).or_default() += amount;
        }
    }

    // Early exit if inputs come from more than one address and none are known gas payers
    if input_addresses.len() > 1 {
        let gas_payer_present = input_addresses.intersection(&gas_payer_addresses).next().is_some();

        if !gas_payer_present {
            return vec![]; // Skip ambiguous multi-input txs
        }

        if input_addresses.len() != 2 {
            return vec![]; // Still ambiguous: multiple non-gas inputs
        }
    }

    // Step 2: Parse outputs
    for output in &tx.unsigned.fixed_outputs {
        let alph_amount = output.atto_alph_amount.parse::<BigDecimal>().unwrap_or_default();
        *output_map.entry((output.address.clone(), ALPH_TOKEN_ID.to_string())).or_default() +=
            alph_amount;

        for token in &output.tokens {
            let amount = token.amount.parse::<BigDecimal>().unwrap_or_default();
            *output_map.entry((output.address.clone(), token.id.clone())).or_default() += amount;
        }
    }

    // Step 3: Detect transfers (from input_map to output_map)
    let dust_amount = BigDecimal::from_str(DUST_AMOUNT).unwrap_or_default();
    let mut transfers = vec![];

    // Filter out gas payers from input_map
    let mut filtered_input_map = input_map.clone();
    for gas_payer in gas_payer_addresses {
        filtered_input_map.retain(|k, _| k.0 != *gas_payer);
    }

    // Use filtered_input_map for processing transfers
    for ((to_addr, token_id), out_amount) in &output_map {
        if token_id == ALPH_TOKEN_ID && out_amount <= &dust_amount {
            continue;
        }

        let input_amount =
            input_map.get(&(to_addr.clone(), token_id.clone())).cloned().unwrap_or_default();
        if input_amount >= *out_amount {
            continue;
        }

        for ((from_addr, in_token_id), in_amount) in &filtered_input_map {
            if in_token_id != token_id || from_addr == to_addr {
                continue;
            }

            if in_amount >= out_amount {
                transfers.push(NewTransferTransactionDto {
                    account_transaction: NewAccountTransaction {
                        address: from_addr.to_string(),
                        tx_type: "transfer".to_string(),
                        from_group: block.chain_from as i16,
                        to_group: block.chain_to as i16,
                        block_height: block.height,
                        tx_id: tx.unsigned.tx_id.to_string(),
                        timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
                    },
                    transfer: NewTransferDetails {
                        token_id: token_id.to_string(),
                        from_address: from_addr.to_string(),
                        to_address: to_addr.to_string(),
                        amount: out_amount.clone(),
                    },
                });
                break;
            }
        }
    }

    transfers
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::FromPrimitive;

    fn load_json_fixture<T: for<'de> serde::Deserialize<'de>>(filename: &str) -> T {
        let path = format!("src/processors/fixtures/{}", filename);
        let json_str = std::fs::read_to_string(&path).expect("Failed to read fixture file");
        serde_json::from_str(&json_str).expect("Failed to parse test JSON")
    }

    fn load_tx_fixture(filename: &str) -> Transaction {
        load_json_fixture(filename)
    }

    fn load_block_fixture(filename: &str) -> RichBlockEntry {
        load_json_fixture(filename)
    }

    fn load_gas_payer_addresses_fixture() -> HashSet<String> {
        HashSet::from(["1C44MNPDY8rNKwaeyrYCF2FPT6rXhNUNtP7fQv81PKjwq".to_string()])
    }

    #[test]
    fn test_extract_transfers_when_alph_transfer() {
        // Given
        let block = load_block_fixture("block_entry.json");
        let transaction = load_tx_fixture("alph_transfer_tx.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert_eq!(
            transfers,
            vec![NewTransferTransactionDto {
                account_transaction: NewAccountTransaction {
                    address: "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E".to_string(),
                    tx_type: "transfer".to_string(),
                    from_group: block.chain_from as i16,
                    to_group: block.chain_to as i16,
                    block_height: block.height,
                    tx_id: "69e487675c435dd99c65d3d5d0b9dcfd8c4d6c7f1cbc94fdc8f960e806c6cd5d"
                        .to_string(),
                    timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
                },
                transfer: NewTransferDetails {
                    token_id: ALPH_TOKEN_ID.to_string(),
                    from_address: "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E".to_string(),
                    to_address: "1CsPJka1BwnLGEEwtKCF9nWLKyRwNwEq5G3Dagij2SyPU".to_string(),
                    amount: BigDecimal::from_i64(500000000000000000).unwrap(),
                },
            }]
        );
    }

    #[test]
    fn test_extract_token_transfer_when_block_reward() {
        // Given
        let block = load_block_fixture("block_entry.json");
        let transaction = load_tx_fixture("block_reward_tx.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert_eq!(transfers, vec![]); // Block rewards should be skipped
    }

    #[test]
    fn test_extract_token_transfer_when_fungible_token_transfer() {
        // Given
        let block = load_block_fixture("block_entry.json");
        let transaction: Transaction = load_tx_fixture("fungible_token_transfer_tx.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert_eq!(
            transfers,
            vec![NewTransferTransactionDto {
                account_transaction: NewAccountTransaction {
                    address: "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E".to_string(),
                    tx_type: "transfer".to_string(),
                    from_group: block.chain_from as i16,
                    to_group: block.chain_to as i16,
                    block_height: block.height,
                    tx_id: "630fefe2f0fca6eb3defcdb665fe1943b5798c0e7507415528ab62ddd01043d6"
                        .to_string(),
                    timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
                },
                transfer: NewTransferDetails {
                    token_id: "bb440a66dcffdb75862b6ad6df14d659aa6d1ba8490f6282708aa44ebc80a100"
                        .to_string(),
                    from_address: "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E".to_string(),
                    to_address: "19tvYk2qzrSnb3SjVzqxE7EaybVrtxEGGpWYDC6dBcsMa".to_string(),
                    amount: BigDecimal::from_i64(1000000000000000).unwrap(),
                },
            }]
        );
    }

    #[test]
    fn test_extract_token_transfer_when_fungible_token_transfer_with_gas_payer() {
        // Given
        let block = load_block_fixture("block_entry.json");
        let transaction: Transaction =
            load_tx_fixture("fungible_token_transfer_with_gas_payer_tx.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert_eq!(
            transfers,
            vec![NewTransferTransactionDto {
                account_transaction: NewAccountTransaction {
                    address: "1CsPJka1BwnLGEEwtKCF9nWLKyRwNwEq5G3Dagij2SyPU".to_string(),
                    tx_type: "transfer".to_string(),
                    from_group: block.chain_from as i16,
                    to_group: block.chain_to as i16,
                    block_height: block.height,
                    tx_id: "10bb1edc2dfc3239f14d0917efb8e9b1aa8e3921a4e36fff9d40fc5ec7cf0ebb"
                        .to_string(),
                    timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
                },
                transfer: NewTransferDetails {
                    token_id: "b2d71c116408ae47b931482a440f675dc9ea64453db24ee931dacd578cae9002"
                        .to_string(),
                    from_address: "1CsPJka1BwnLGEEwtKCF9nWLKyRwNwEq5G3Dagij2SyPU".to_string(),
                    to_address: "1EJCtZP3HZP5rDX5v2o32woqLTxp6GS4GoLQGpzVPQm6E".to_string(),
                    amount: BigDecimal::from_i64(2).unwrap(),
                },
            }]
        );
    }

    #[test]
    fn test_extract_token_transfer_when_alph_transfer_with_gas_payer() {
        // Given
        let transaction: Transaction = load_tx_fixture("alph_transfer_with_gas_payer_tx.json");
        let block = load_block_fixture("block_entry.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert_eq!(
            transfers,
            vec![NewTransferTransactionDto {
                account_transaction: NewAccountTransaction {
                    address: "1CsPJka1BwnLGEEwtKCF9nWLKyRwNwEq5G3Dagij2SyPU".to_string(),
                    tx_type: "transfer".to_string(),
                    from_group: block.chain_from as i16,
                    to_group: block.chain_to as i16,
                    block_height: block.height,
                    tx_id: "33fb4ca98b33b57e063298d88acf45bf95ec10d41c43b90e3b8fb3dbfed4ad1f"
                        .to_string(),
                    timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
                },
                transfer: NewTransferDetails {
                    token_id: ALPH_TOKEN_ID.to_string(),
                    from_address: "1CsPJka1BwnLGEEwtKCF9nWLKyRwNwEq5G3Dagij2SyPU".to_string(),
                    to_address: "18Bf8JMSqF6MXVNKpsYo3zpfhob2q2snvu3Df5EEUZ74A".to_string(),
                    amount: BigDecimal::from_i64(50000000000000000).unwrap(),
                },
            }]
        );
    }

    #[test]
    fn test_extract_transfers_when_multiple_token_transfers() {
        // Given
        let transaction = load_tx_fixture("multiple_token_transfers_tx.json");
        let block = load_block_fixture("block_entry.json");
        let gas_payer_addresses = load_gas_payer_addresses_fixture();

        // When
        let transfers = extract_token_transfers(&transaction, &block, &gas_payer_addresses);

        // Then
        assert!(
            transfers.iter().any(|el| el.transfer.token_id
                == "6b894505030718e45cdf7c59be1f8c6167542e43522e95303871e8280037b000"
                && el.transfer.from_address == "19sJ8t5rtjHyJKjYAQ5ndbwxpjc7q5aLGEGF1mjw4cfZ4"
                && el.transfer.to_address == "13yjxHVqCPZmw2AwcbhchaegL4YoKXdQP6oLFvJhF4Zqw"
                && el.transfer.amount == BigDecimal::from_i64(10000000000).unwrap()
                && el.account_transaction.tx_id
                    == "cdccdd80af1acbbf649028fca799ad1e8bd01dde03fa13b7f43a6ae37668201f"),
            "Expected transfer not found in results"
        );
        assert!(
            transfers.iter().any(|el| el.transfer.token_id
                == "cad22f7c98f13fe249c25199c61190a9fb4341f8af9b1c17fcff4cd4b2c3d200"
                && el.transfer.from_address == "19sJ8t5rtjHyJKjYAQ5ndbwxpjc7q5aLGEGF1mjw4cfZ4"
                && el.transfer.to_address == "13yjxHVqCPZmw2AwcbhchaegL4YoKXdQP6oLFvJhF4Zqw"
                && el.transfer.amount == BigDecimal::from_i64(100000000000000000).unwrap()
                && el.account_transaction.tx_id
                    == "cdccdd80af1acbbf649028fca799ad1e8bd01dde03fa13b7f43a6ae37668201f"),
            "Expected transfer not found in results"
        );
        assert!(
            transfers.iter().any(|el| el.transfer.token_id == ALPH_TOKEN_ID
                && el.transfer.from_address == "19sJ8t5rtjHyJKjYAQ5ndbwxpjc7q5aLGEGF1mjw4cfZ4"
                && el.transfer.to_address == "13yjxHVqCPZmw2AwcbhchaegL4YoKXdQP6oLFvJhF4Zqw"
                && el.transfer.amount == BigDecimal::from_i64(100000000000000000).unwrap()
                && el.account_transaction.tx_id
                    == "cdccdd80af1acbbf649028fca799ad1e8bd01dde03fa13b7f43a6ae37668201f"),
            "Expected transfer not found in results"
        );
    }
}
