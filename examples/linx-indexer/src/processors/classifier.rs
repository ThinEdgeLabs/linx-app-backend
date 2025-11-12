use bento_types::Transaction;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionCategory {
    Transfer,
    Swap,
    ContractCall,
    BlockReward,
    Unknown,
}

pub struct TransactionClassifier {
    pub dex_contract_addresses: HashSet<String>,
}

impl TransactionClassifier {
    pub fn new(dex_addresses: HashSet<String>) -> Self {
        Self { dex_contract_addresses: dex_addresses }
    }

    pub fn classify(&self, tx: &Transaction) -> TransactionCategory {
        if self.is_block_reward(tx) {
            return TransactionCategory::BlockReward;
        }

        if self.is_swap_transaction(tx) {
            return TransactionCategory::Swap;
        }

        if self.is_contract_transaction(tx) {
            return TransactionCategory::ContractCall;
        }

        TransactionCategory::Transfer
    }

    fn is_swap_transaction(&self, tx: &Transaction) -> bool {
        tx.contract_inputs.iter().any(|input| self.dex_contract_addresses.contains(&input.address))
            || tx
                .generated_outputs
                .iter()
                .any(|output| self.dex_contract_addresses.contains(&output.address))
    }

    fn is_contract_transaction(&self, tx: &Transaction) -> bool {
        !tx.contract_inputs.is_empty() || !tx.generated_outputs.is_empty()
    }

    fn is_block_reward(&self, tx: &Transaction) -> bool {
        tx.unsigned.inputs.is_empty() && tx.unsigned.fixed_outputs.len() == 1
    }
}
