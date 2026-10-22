use std::{collections::HashMap, fmt::Debug, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use bento_core::{ProcessorFactory, db::DbPool};
use bento_trait::processor::ProcessorTrait;
use bento_types::{
    BlockAndEvents, ContractEventByBlockHash, CustomProcessorOutput, EventField, RichBlockEntry,
    processors::ProcessorOutput, utils::timestamp_millis_to_naive_datetime,
};
use bigdecimal::{BigDecimal, Zero};

use crate::{
    address_from_contract_id,
    models::{NewAccountTransaction, NewPoolDto, Pool, SwapDetails},
    repository::{AccountTransactionRepository, PoolRepository},
};

const AYIN_V2_FACTORY_ADDRESS: &str = "vyrkJHG49TXss6pGAz2dVxq5o7mBXNNXAV18nAeqVT1R";
const ELEXIUM_FACTORY_ADDRESS: &str = "22oTtDJEMjNc9QAdmcZarnEzgkAooJp9gZy7RYBisniR5";
const POWFI_CPMM_FACTORY_ADDRESS: &str = "24CXRLvj1QiDH6riwr1EPERG7BcduHKZtktygbq97MfYb";
const POWFI_CLMM_FACTORY_ADDRESS: &str = "24gqd4DMQXVzjm5QDAGxUtYupx3WM2PSu3vZUseuBCV9h";

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, config: Option<Arc<dyn bento_types::config::AppConfigTrait>>| {
        let processor = DexProcessor::new(db_pool, config);
        Box::new(processor)
    }
}

pub struct DexProcessor {
    connection_pool: Arc<DbPool>,
    swap_repository: AccountTransactionRepository,
    pool_repository: PoolRepository,
}

impl DexProcessor {
    pub fn new(connection_pool: Arc<DbPool>, _config: Option<Arc<dyn bento_types::config::AppConfigTrait>>) -> Self {
        let swap_repository = AccountTransactionRepository::new(connection_pool.clone());
        let pool_repository = PoolRepository::new(connection_pool.clone());

        Self { connection_pool, swap_repository, pool_repository }
    }
}

fn extract_new_pools(events: &[ContractEventByBlockHash]) -> Vec<NewPoolDto> {
    events.iter().filter_map(parse_pool_creation_event).collect()
}

/// Parse pool creation event for different DEX factories
fn parse_pool_creation_event(event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
    match event.contract_address.as_str() {
        AYIN_V2_FACTORY_ADDRESS if event.event_index == 0 => {
            parse_v2_pair_created_event(event, AYIN_V2_FACTORY_ADDRESS)
        }
        POWFI_CPMM_FACTORY_ADDRESS if event.event_index == 0 => {
            parse_v2_pair_created_event(event, POWFI_CPMM_FACTORY_ADDRESS)
        }
        ELEXIUM_FACTORY_ADDRESS if event.event_index == 0 => parse_elexium_pool_event(event),
        POWFI_CLMM_FACTORY_ADDRESS if event.event_index == 1 => parse_powfi_clmm_pool_event(event),
        _ => None,
    }
}

/// Parse a constant-product `PairCreated(token0, token1, pair, currentPairSize)` event
fn parse_v2_pair_created_event(event: &ContractEventByBlockHash, factory_address: &str) -> Option<NewPoolDto> {
    if event.fields.len() < 4 {
        tracing::warn!("Pair creation event has insufficient fields: {}", event.fields.len());
        return None;
    }

    let token_a = extract_string_field(&event.fields, 0)?;
    let token_b = extract_string_field(&event.fields, 1)?;
    let contract_id = extract_string_field(&event.fields, 2)?;

    tracing::debug!("Parsed pair: token_a={}, token_b={}, contract_id={}", token_a, token_b, contract_id);

    Some(NewPoolDto {
        address: address_from_contract_id(&contract_id),
        token_a,
        token_b,
        factory_address: factory_address.to_string(),
    })
}

/// Parse Elexium pool creation event
fn parse_elexium_pool_event(event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
    if event.fields.len() < 5 {
        tracing::warn!("Elexium pool creation event has insufficient fields: {}", event.fields.len());
        return None;
    }

    let token_a = extract_string_field(&event.fields, 0)?;
    let token_b = extract_string_field(&event.fields, 1)?;
    let contract_id = extract_string_field(&event.fields, 3)?;

    tracing::debug!("Parsed Elexium pool: token_a={}, token_b={}, contract_id={}", token_a, token_b, contract_id);

    Some(NewPoolDto {
        address: address_from_contract_id(&contract_id),
        token_a,
        token_b,
        factory_address: ELEXIUM_FACTORY_ADDRESS.to_string(),
    })
}

/// Parse a PowFi concentrated liquidity `PoolCreated(id, token0, token1, configIndex)` event
fn parse_powfi_clmm_pool_event(event: &ContractEventByBlockHash) -> Option<NewPoolDto> {
    if event.fields.len() < 4 {
        tracing::warn!("PowFi CLMM pool creation event has insufficient fields: {}", event.fields.len());
        return None;
    }

    let contract_id = extract_string_field(&event.fields, 0)?;
    let token_a = extract_string_field(&event.fields, 1)?;
    let token_b = extract_string_field(&event.fields, 2)?;

    tracing::debug!("Parsed PowFi CLMM pool: token_a={}, token_b={}, contract_id={}", token_a, token_b, contract_id);

    Some(NewPoolDto {
        address: address_from_contract_id(&contract_id),
        token_a,
        token_b,
        factory_address: POWFI_CLMM_FACTORY_ADDRESS.to_string(),
    })
}

/// Returns each swap together with the position of its event within the block
fn extract_swaps(bwe: &BlockAndEvents, pools: &HashMap<String, Pool>) -> Vec<(usize, NewAccountTransaction)> {
    bwe.events
        .iter()
        .enumerate()
        .filter_map(|(position, event)| parse_swap_event(event, pools, &bwe.block).map(|swap| (position, swap)))
        .collect()
}

fn parse_swap_event(
    event: &ContractEventByBlockHash,
    pools: &HashMap<String, Pool>,
    block: &RichBlockEntry,
) -> Option<NewAccountTransaction> {
    let pool = pools.get(event.contract_address.as_str())?;
    match pool.factory_address.as_str() {
        AYIN_V2_FACTORY_ADDRESS | POWFI_CPMM_FACTORY_ADDRESS => parse_v2_swap_event(event, pool, block, 2),
        ELEXIUM_FACTORY_ADDRESS => parse_v2_swap_event(event, pool, block, 3),
        POWFI_CLMM_FACTORY_ADDRESS => parse_powfi_clmm_swap_event(event, pool, block),
        _ => None,
    }
}

/// Parse a constant-product `Swap(sender, amount0In, amount1In, amount0Out, amount1Out, to)` event
fn parse_v2_swap_event(
    event: &ContractEventByBlockHash,
    pool: &Pool,
    block: &RichBlockEntry,
    swap_event_index: i32,
) -> Option<NewAccountTransaction> {
    if event.fields.len() != 6 || event.event_index != swap_event_index {
        return None;
    }

    let sender = crate::normalize_address(&extract_string_field(&event.fields, 0)?).to_string();
    let token_a_in = extract_bigdecimal_field(&event.fields, 1)?;
    let token_b_in = extract_bigdecimal_field(&event.fields, 2)?;
    let token_a_out = extract_bigdecimal_field(&event.fields, 3)?;
    let token_b_out = extract_bigdecimal_field(&event.fields, 4)?;

    // Determine swap direction
    let (token_in, token_out, amount_in, amount_out) = if token_a_in.is_zero() {
        (&pool.token_b, &pool.token_a, token_b_in, token_a_out)
    } else {
        (&pool.token_a, &pool.token_b, token_a_in, token_b_out)
    };

    build_swap_transaction(sender, token_in, token_out, amount_in, amount_out, event, block)
}

/// Parse a PowFi concentrated liquidity
/// `Swap(sender, recipient, amount0, amount1, sqrtPriceX96, liquidity, tick)` event.
///
/// `amount0` and `amount1` are signed: positive is paid into the pool, negative is paid out.
fn parse_powfi_clmm_swap_event(
    event: &ContractEventByBlockHash,
    pool: &Pool,
    block: &RichBlockEntry,
) -> Option<NewAccountTransaction> {
    if event.fields.len() != 7 || event.event_index != 3 {
        return None;
    }

    let sender = crate::normalize_address(&extract_string_field(&event.fields, 0)?).to_string();
    let amount0 = extract_bigdecimal_field(&event.fields, 2)?;
    let amount1 = extract_bigdecimal_field(&event.fields, 3)?;

    let (token_in, token_out, amount_in, amount_out) = if amount0 > BigDecimal::zero() {
        (&pool.token_a, &pool.token_b, amount0, -amount1)
    } else {
        (&pool.token_b, &pool.token_a, amount1, -amount0)
    };

    build_swap_transaction(sender, token_in, token_out, amount_in, amount_out, event, block)
}

fn build_swap_transaction(
    sender: String,
    token_in: &str,
    token_out: &str,
    amount_in: BigDecimal,
    amount_out: BigDecimal,
    event: &ContractEventByBlockHash,
    block: &RichBlockEntry,
) -> Option<NewAccountTransaction> {
    let swap = SwapDetails {
        token_in: token_in.to_string(),
        token_out: token_out.to_string(),
        amount_in,
        amount_out,
        pool_address: event.contract_address.clone(),
        tx_id: event.tx_id.to_string(),
        hop_count: 1,
    };

    Some(NewAccountTransaction {
        address: sender,
        tx_type: "swap".to_string(),
        tx_id: event.tx_id.to_string(),
        from_group: block.chain_from as i16,
        to_group: block.chain_to as i16,
        block_height: block.height,
        timestamp: timestamp_millis_to_naive_datetime(block.timestamp),
        details: serde_json::to_value(&swap).ok()?,
    })
}

fn extract_string_field(fields: &[EventField], index: usize) -> Option<String> {
    fields.get(index)?.value.as_str().map(|s| s.to_string())
}

fn extract_bigdecimal_field(fields: &[EventField], index: usize) -> Option<BigDecimal> {
    fields.get(index)?.value.as_str().and_then(|s| s.parse().ok())
}

fn get_swap_details(account_tx: &NewAccountTransaction) -> Option<SwapDetails> {
    serde_json::from_value(account_tx.details.clone()).ok()
}

fn set_swap_details(mut account_tx: NewAccountTransaction, swap: SwapDetails) -> NewAccountTransaction {
    account_tx.details = serde_json::to_value(&swap).unwrap();
    account_tx
}

/// Orders the hops of a multi-hop swap so that each hop's `token_out` is the next hop's `token_in`.
///
/// Returns `None` when the hops don't form a single unambiguous chain.
fn order_hops_by_token_chain(hops: &[SwapDetails]) -> Option<Vec<usize>> {
    let mut starts = (0..hops.len()).filter(|&i| !hops.iter().any(|other| other.token_out == hops[i].token_in));
    let start = starts.next()?;
    if starts.next().is_some() {
        return None;
    }

    let mut order = vec![start];
    while order.len() < hops.len() {
        let current = &hops[*order.last()?];
        let mut next = (0..hops.len()).filter(|i| !order.contains(i) && hops[*i].token_in == current.token_out);
        let next_hop = next.next()?;
        if next.next().is_some() {
            return None;
        }
        order.push(next_hop);
    }

    Some(order)
}

/// Aggregate multi-hop and split swaps by transaction ID
///
/// Groups swaps by tx_id and creates aggregated swaps.
///
/// Handles two scenarios:
/// 1. **Multi-hop swaps**: Chained swaps where tokens connect (A → B → C)
///    - Hops are ordered by following each hop's token_out to the next hop's token_in, because pools call
///      the next pool before emitting their own swap event, so event order doesn't follow the route.
///      If the hops don't form one unambiguous chain (a cycle such as A → B → C → A, or independent
///      swaps batched in one transaction), they are taken in the order their events were emitted
///    - Aggregated shows: first token_in and last token_out, attributed to the sender of the first hop
///    - Example: ALPH → USDT → TOKEN becomes ALPH → TOKEN
///    - hop_count indicates number of hops (e.g., 2 for a 2-hop swap)
///
/// 2. **Split swaps**: Same token pair split across multiple pools
///    - Aggregated shows: same tokens with summed amounts
///    - Example: 100 A → 50 B (pool1) + 100 A → 50 B (pool2) = 200 A → 100 B
///    - hop_count indicates number of pools used
///
/// Returns only aggregated swaps (individual hops are not stored).
fn aggregate_multi_hop_swaps(swaps_with_position: Vec<(usize, NewAccountTransaction)>) -> Vec<NewAccountTransaction> {
    // Group swaps by tx_id
    let mut swaps_by_tx: HashMap<String, Vec<(usize, NewAccountTransaction)>> = HashMap::new();

    for (position, swap) in swaps_with_position {
        swaps_by_tx.entry(swap.tx_id.clone()).or_default().push((position, swap));
    }

    let mut result = Vec::new();

    for (tx_id, mut swap_list) in swaps_by_tx {
        if swap_list.len() == 1 {
            // Single swap - no aggregation needed
            result.push(swap_list.into_iter().next().unwrap().1);
            continue;
        }

        swap_list.sort_by_key(|(position, _)| *position);
        let swaps: Vec<NewAccountTransaction> = swap_list.into_iter().map(|(_, swap)| swap).collect();
        let Some(details): Option<Vec<SwapDetails>> = swaps.iter().map(get_swap_details).collect() else {
            tracing::warn!("Skipping swaps for tx_id {} with unreadable swap details", tx_id);
            continue;
        };

        let hop_count = details.len() as i32;

        // Detect if this is a split swap (same token pair) or multi-hop
        let is_split_swap =
            details.iter().all(|d| d.token_in == details[0].token_in && d.token_out == details[0].token_out);

        let aggregated = if is_split_swap {
            // Split swap: same token pair, sum amounts
            tracing::debug!("Aggregating split swap for tx_id {} across {} pools", tx_id, hop_count);

            let aggregated_swap = SwapDetails {
                token_in: details[0].token_in.clone(),
                token_out: details[0].token_out.clone(),
                amount_in: details.iter().map(|d| &d.amount_in).sum(),
                amount_out: details.iter().map(|d| &d.amount_out).sum(),
                pool_address: details.iter().map(|d| d.pool_address.as_str()).collect::<Vec<_>>().join(","),
                tx_id: tx_id.clone(),
                hop_count,
            };

            set_swap_details(swaps[0].clone(), aggregated_swap)
        } else {
            // Multi-hop swap: chained tokens, use first and last
            let order = order_hops_by_token_chain(&details).unwrap_or_else(|| {
                tracing::warn!(
                    "Could not chain the {} hops of tx_id {} by token, falling back to event order",
                    hop_count,
                    tx_id
                );
                (0..details.len()).collect()
            });
            tracing::debug!("Aggregating {}-hop swap for tx_id {}", hop_count, tx_id);

            let first = &details[order[0]];
            let last = &details[order[order.len() - 1]];

            let aggregated_swap = SwapDetails {
                token_in: first.token_in.clone(),
                token_out: last.token_out.clone(),
                amount_in: first.amount_in.clone(),
                amount_out: last.amount_out.clone(),
                pool_address: order.iter().map(|&i| details[i].pool_address.as_str()).collect::<Vec<_>>().join(","),
                tx_id: tx_id.clone(),
                hop_count,
            };

            set_swap_details(swaps[order[0]].clone(), aggregated_swap)
        };

        // Add only the aggregated swap (individual hops are not stored)
        result.push(aggregated);
    }

    result
}

#[derive(Debug, Clone)]
pub struct DexProcessorOutput {
    pub new_pools: Vec<NewPoolDto>,
    pub swaps: Vec<NewAccountTransaction>,
}

impl CustomProcessorOutput for DexProcessorOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

impl Debug for DexProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DexProcessor")
    }
}

#[async_trait]
impl ProcessorTrait for DexProcessor {
    fn name(&self) -> &'static str {
        "dex"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, bwe: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        let mut swaps = Vec::new();
        // This might be an issue if the number of pools is large
        let mut existing_pools = self.pool_repository.get_pools().await?;
        let mut new_pools: Vec<NewPoolDto> = Vec::new();

        for block_events in &bwe {
            let block_pools = extract_new_pools(&block_events.events);
            existing_pools.extend(block_pools.iter().map(|p| (p.address.clone(), Pool::from(p.clone()))));
            new_pools.extend(block_pools);

            let block_swaps = extract_swaps(block_events, &existing_pools);
            swaps.extend(block_swaps);
        }

        // Aggregate multi-hop swaps
        let processed_swaps = aggregate_multi_hop_swaps(swaps);

        tracing::info!(
            "Processed {} blocks with {} new pools and {} swaps (after aggregation)",
            bwe.len(),
            new_pools.len(),
            processed_swaps.len()
        );

        Ok(ProcessorOutput::Custom(Arc::new(DexProcessorOutput { new_pools, swaps: processed_swaps })))
    }

    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output
            && let Some(dex_output) = custom.as_any().downcast_ref::<DexProcessorOutput>()
        {
            if !dex_output.new_pools.is_empty() {
                self.pool_repository.insert_pools(&dex_output.new_pools).await?;
                tracing::info!("Inserted {} new pools", dex_output.new_pools.len());
            }

            if !dex_output.swaps.is_empty() {
                self.swap_repository.insert_transactions(&dex_output.swaps).await?;
                tracing::info!("Inserted {} swaps", dex_output.swaps.len());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bento_types::EventFieldType;

    const TOKEN_A: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const TOKEN_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const TOKEN_C: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const USER: &str = "1DrDyTr9RpRsQnDnXo2YRiPzPW4ooHX5LLoqXrqfMrpQH";
    const TX_ID: &str = "abababababababababababababababababababababababababababababababab";

    fn load_block_fixture() -> RichBlockEntry {
        let json_str =
            std::fs::read_to_string("src/processors/fixtures/block_entry.json").expect("Failed to read fixture file");
        serde_json::from_str(&json_str).expect("Failed to parse test JSON")
    }

    fn field(field_type: EventFieldType, value: &str) -> EventField {
        EventField { field_type, value: serde_json::Value::String(value.to_string()) }
    }

    fn event(contract_address: &str, event_index: i32, fields: Vec<EventField>) -> ContractEventByBlockHash {
        ContractEventByBlockHash {
            tx_id: TX_ID.to_string(),
            contract_address: contract_address.to_string(),
            event_index,
            fields,
        }
    }

    fn pool(address: &str, token_a: &str, token_b: &str, factory_address: &str) -> Pool {
        Pool {
            id: 0,
            address: address.to_string(),
            token_a: token_a.to_string(),
            token_b: token_b.to_string(),
            factory_address: factory_address.to_string(),
        }
    }

    fn pools(list: Vec<Pool>) -> HashMap<String, Pool> {
        list.into_iter().map(|p| (p.address.clone(), p)).collect()
    }

    fn clmm_swap_event(pool_address: &str, sender: &str, amount0: &str, amount1: &str) -> ContractEventByBlockHash {
        event(
            pool_address,
            3,
            vec![
                field(EventFieldType::Address, sender),
                field(EventFieldType::Address, USER),
                field(EventFieldType::I256, amount0),
                field(EventFieldType::I256, amount1),
                field(EventFieldType::U256, "79228162514264337593543950336"),
                field(EventFieldType::U256, "1000000"),
                field(EventFieldType::I256, "0"),
            ],
        )
    }

    fn v2_swap_event(
        pool_address: &str,
        event_index: i32,
        sender: &str,
        amounts: [&str; 4],
    ) -> ContractEventByBlockHash {
        event(
            pool_address,
            event_index,
            vec![
                field(EventFieldType::Address, sender),
                field(EventFieldType::U256, amounts[0]),
                field(EventFieldType::U256, amounts[1]),
                field(EventFieldType::U256, amounts[2]),
                field(EventFieldType::U256, amounts[3]),
                field(EventFieldType::Address, USER),
            ],
        )
    }

    fn swaps_for(events: Vec<ContractEventByBlockHash>, pools: &HashMap<String, Pool>) -> Vec<NewAccountTransaction> {
        let bwe = BlockAndEvents { block: load_block_fixture(), events };
        aggregate_multi_hop_swaps(extract_swaps(&bwe, pools))
    }

    fn details(swap: &NewAccountTransaction) -> SwapDetails {
        get_swap_details(swap).unwrap()
    }

    fn decimal(value: &str) -> BigDecimal {
        value.parse().unwrap()
    }

    #[test]
    fn powfi_cpmm_pair_created_registers_pool_under_powfi_factory() {
        // Given
        let contract_id = "33".repeat(32);
        let event = event(
            POWFI_CPMM_FACTORY_ADDRESS,
            0,
            vec![
                field(EventFieldType::ByteVec, TOKEN_A),
                field(EventFieldType::ByteVec, TOKEN_B),
                field(EventFieldType::ByteVec, &contract_id),
                field(EventFieldType::U256, "1"),
            ],
        );

        // When
        let pools = extract_new_pools(&[event]);

        // Then
        assert_eq!(
            pools,
            vec![NewPoolDto {
                token_a: TOKEN_A.to_string(),
                token_b: TOKEN_B.to_string(),
                factory_address: POWFI_CPMM_FACTORY_ADDRESS.to_string(),
                address: address_from_contract_id(&contract_id),
            }]
        );
    }

    #[test]
    fn powfi_clmm_pool_created_uses_contract_id_from_first_field() {
        // Given
        let contract_id = "44".repeat(32);
        let event = event(
            POWFI_CLMM_FACTORY_ADDRESS,
            1,
            vec![
                field(EventFieldType::ByteVec, &contract_id),
                field(EventFieldType::ByteVec, TOKEN_A),
                field(EventFieldType::ByteVec, TOKEN_B),
                field(EventFieldType::U256, "2"),
            ],
        );

        // When
        let pools = extract_new_pools(&[event]);

        // Then
        assert_eq!(
            pools,
            vec![NewPoolDto {
                token_a: TOKEN_A.to_string(),
                token_b: TOKEN_B.to_string(),
                factory_address: POWFI_CLMM_FACTORY_ADDRESS.to_string(),
                address: address_from_contract_id(&contract_id),
            }]
        );
    }

    #[test]
    fn powfi_clmm_factory_config_events_are_not_pools() {
        // Given
        let event = event(POWFI_CLMM_FACTORY_ADDRESS, 0, vec![field(EventFieldType::U256, "0")]);

        // When
        let pools = extract_new_pools(&[event]);

        // Then
        assert!(pools.is_empty());
    }

    #[test]
    fn powfi_clmm_swap_with_positive_amount0_sells_token_a() {
        // Given
        let pools = pools(vec![pool("clmm-pool", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS)]);
        let events = vec![clmm_swap_event("clmm-pool", USER, "1000", "-990")];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        assert_eq!(swaps[0].address, USER);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_B);
        assert_eq!(swap.amount_in, decimal("1000"));
        assert_eq!(swap.amount_out, decimal("990"));
        assert_eq!(swap.hop_count, 1);
    }

    #[test]
    fn powfi_clmm_swap_with_positive_amount1_sells_token_b() {
        // Given
        let pools = pools(vec![pool("clmm-pool", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS)]);
        let events = vec![clmm_swap_event("clmm-pool", USER, "-495", "500")];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_B);
        assert_eq!(swap.token_out, TOKEN_A);
        assert_eq!(swap.amount_in, decimal("500"));
        assert_eq!(swap.amount_out, decimal("495"));
    }

    #[test]
    fn powfi_cpmm_swap_is_parsed_like_a_v2_swap() {
        // Given
        let pools = pools(vec![pool("cpmm-pool", TOKEN_A, TOKEN_B, POWFI_CPMM_FACTORY_ADDRESS)]);
        let events = vec![v2_swap_event("cpmm-pool", 2, USER, ["0", "300", "290", "0"])];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_B);
        assert_eq!(swap.token_out, TOKEN_A);
        assert_eq!(swap.amount_in, decimal("300"));
        assert_eq!(swap.amount_out, decimal("290"));
    }

    #[test]
    fn clmm_exact_input_two_hop_reports_user_and_first_to_last_tokens() {
        // Given: A → B in pool-ab, then B → C in pool-bc. pool-ab sends its B output straight into pool-bc, so
        // pool-bc's swap event has pool-ab as sender. pool-bc's event is emitted first because pool-ab calls
        // pool-bc before emitting its own event.
        let pools = pools(vec![
            pool("pool-ab", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS),
            pool("pool-bc", TOKEN_B, TOKEN_C, POWFI_CLMM_FACTORY_ADDRESS),
        ]);
        let events = vec![
            clmm_swap_event("pool-bc", "pool-ab", "900", "-800"),
            clmm_swap_event("pool-ab", USER, "1000", "-900"),
        ];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        assert_eq!(swaps[0].address, USER);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_C);
        assert_eq!(swap.amount_in, decimal("1000"));
        assert_eq!(swap.amount_out, decimal("800"));
        assert_eq!(swap.pool_address, "pool-ab,pool-bc");
        assert_eq!(swap.hop_count, 2);
    }

    #[test]
    fn exact_output_two_hop_orders_hops_by_token_chain() {
        // Given: A → B in pool-ab, then B → C in pool-bc. Exact output routes start at the output pool, so the
        // first hop's event is emitted first.
        let pools = pools(vec![
            pool("pool-ab", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS),
            pool("pool-bc", TOKEN_B, TOKEN_C, POWFI_CLMM_FACTORY_ADDRESS),
        ]);
        let events =
            vec![clmm_swap_event("pool-ab", USER, "1000", "-900"), clmm_swap_event("pool-bc", USER, "900", "-800")];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_C);
        assert_eq!(swap.amount_in, decimal("1000"));
        assert_eq!(swap.amount_out, decimal("800"));
        assert_eq!(swap.pool_address, "pool-ab,pool-bc");
    }

    #[test]
    fn mixed_cpmm_and_clmm_route_orders_hops_by_token_chain() {
        // Given: A → B in a constant-product pair, then B → C in a concentrated liquidity pool. The CLMM swap
        // has a higher event index than the CPMM swap, which must not affect the hop order.
        let pools = pools(vec![
            pool("cpmm-ab", TOKEN_A, TOKEN_B, POWFI_CPMM_FACTORY_ADDRESS),
            pool("clmm-bc", TOKEN_B, TOKEN_C, POWFI_CLMM_FACTORY_ADDRESS),
        ]);
        let events = vec![
            v2_swap_event("cpmm-ab", 2, USER, ["1000", "0", "0", "900"]),
            clmm_swap_event("clmm-bc", USER, "900", "-800"),
        ];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_C);
        assert_eq!(swap.amount_in, decimal("1000"));
        assert_eq!(swap.amount_out, decimal("800"));
        assert_eq!(swap.pool_address, "cpmm-ab,clmm-bc");
    }

    #[test]
    fn route_back_to_the_starting_token_keeps_event_order() {
        // Given: A → B → C → A, which has no hop whose token_in isn't another hop's token_out
        let pools = pools(vec![
            pool("pool-ab", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS),
            pool("pool-bc", TOKEN_B, TOKEN_C, POWFI_CLMM_FACTORY_ADDRESS),
            pool("pool-ac", TOKEN_A, TOKEN_C, POWFI_CLMM_FACTORY_ADDRESS),
        ]);
        let events = vec![
            clmm_swap_event("pool-ab", USER, "1000", "-900"),
            clmm_swap_event("pool-bc", USER, "900", "-800"),
            clmm_swap_event("pool-ac", USER, "-1010", "800"),
        ];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_A);
        assert_eq!(swap.amount_in, decimal("1000"));
        assert_eq!(swap.amount_out, decimal("1010"));
        assert_eq!(swap.pool_address, "pool-ab,pool-bc,pool-ac");
        assert_eq!(swap.hop_count, 3);
    }

    #[test]
    fn split_swap_across_pools_sums_amounts() {
        // Given
        let pools = pools(vec![
            pool("ayin-ab", TOKEN_A, TOKEN_B, AYIN_V2_FACTORY_ADDRESS),
            pool("clmm-ab", TOKEN_A, TOKEN_B, POWFI_CLMM_FACTORY_ADDRESS),
        ]);
        let events = vec![
            v2_swap_event("ayin-ab", 2, USER, ["100", "0", "0", "50"]),
            clmm_swap_event("clmm-ab", USER, "100", "-51"),
        ];

        // When
        let swaps = swaps_for(events, &pools);

        // Then
        assert_eq!(swaps.len(), 1);
        let swap = details(&swaps[0]);
        assert_eq!(swap.token_in, TOKEN_A);
        assert_eq!(swap.token_out, TOKEN_B);
        assert_eq!(swap.amount_in, decimal("200"));
        assert_eq!(swap.amount_out, decimal("101"));
        assert_eq!(swap.hop_count, 2);
    }
}
