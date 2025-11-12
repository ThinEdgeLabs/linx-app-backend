use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bento_core::ProcessorFactory;
use bento_core::db::DbPool;
use bento_trait::processor::ProcessorTrait;
use bento_types::BlockAndEvents;
use bento_types::ContractEventByBlockHash;
use bento_types::CustomProcessorOutput;
use bento_types::processors::ProcessorOutput;
use bento_types::utils::timestamp_millis_to_naive_datetime;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::NaiveDateTime;
use diesel::FromSqlRow;
use diesel::expression::AsExpression;
use diesel::insert_into;
use diesel::prelude::*;
use diesel::sql_types::SmallInt;
use diesel_async::RunQueryDsl;
use diesel_enum::DbEnum;
use serde::Serialize;

pub fn processor_factory() -> ProcessorFactory {
    |db_pool, args: Option<serde_json::Value>| {
        Box::new(LendingContractProcessor::new(db_pool, args.unwrap_or_default()))
    }
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone, Serialize, AsChangeset)]
#[diesel(table_name = bento_types::schema::loan_actions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct LoanActionModel {
    loan_subcontract_id: String,
    loan_id: Option<BigDecimal>,
    by: String,
    timestamp: NaiveDateTime,
    action_type: LoanActionType,
}

#[derive(Queryable, Selectable, Insertable, Debug, Clone, Serialize, AsChangeset)]
#[diesel(table_name = bento_types::schema::loan_details)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct LoanDetailModel {
    loan_subcontract_id: String,
    lending_token_id: String,
    collateral_token_id: String,
    lending_amount: BigDecimal,
    collateral_amount: BigDecimal,
    interest_rate: BigDecimal,
    duration: BigDecimal,
    lender: String,
}

pub struct LendingContractProcessor {
    connection_pool: Arc<DbPool>,
    contract_address: String,
}

impl LendingContractProcessor {
    pub fn new(connection_pool: Arc<DbPool>, args: serde_json::Value) -> Self {
        let contract_address = args
            .get("contract_address")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("Missing contract address argument"))
            .to_string();
        Self { connection_pool, contract_address }
    }
}

impl Debug for LendingContractProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = &self.connection_pool.state();
        write!(
            f,
            "LoanActionProcessor {{ connections: {:?}  idle_connections: {:?} }}",
            state.connections, state.idle_connections
        )
    }
}

#[derive(Debug, Clone)]
pub struct LendingContractOutput {
    pub loan_actions: Vec<LoanActionModel>,
    pub loan_details: Vec<LoanDetailModel>,
}

impl CustomProcessorOutput for LendingContractOutput {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn clone_box(&self) -> Box<dyn CustomProcessorOutput> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl ProcessorTrait for LendingContractProcessor {
    fn name(&self) -> &'static str {
        "lending"
    }

    fn connection_pool(&self) -> &Arc<DbPool> {
        &self.connection_pool
    }

    async fn process_blocks(&self, blocks: Vec<BlockAndEvents>) -> Result<ProcessorOutput> {
        // Process blocks and convert to models
        let (loan_actions, loan_details) = convert_to_model(blocks, &self.contract_address);

        tracing::info!(
            "Processed {} loan actions and {} loan details",
            loan_actions.len(),
            loan_details.len()
        );

        // Return custom output
        Ok(ProcessorOutput::Custom(Arc::new(LendingContractOutput { loan_actions, loan_details })))
    }

    // Override storage method to handle our custom output
    async fn store_output(&self, output: ProcessorOutput) -> Result<()> {
        if let ProcessorOutput::Custom(custom) = output {
            // Downcast to our specific output type
            if let Some(lending_output) = custom.as_any().downcast_ref::<LendingContractOutput>() {
                let loan_actions = &lending_output.loan_actions;
                let loan_details = &lending_output.loan_details;

                // Store loan actions
                if !loan_actions.is_empty() {
                    insert_loan_actions_to_db(self.connection_pool.clone(), loan_actions.clone())
                        .await?;
                }

                // Store loan details
                if !loan_details.is_empty() {
                    insert_loan_details_to_db(self.connection_pool.clone(), loan_details.clone())
                        .await?;
                }

                tracing::info!(
                    "Stored {} loan actions and {} loan details",
                    loan_actions.len(),
                    loan_details.len()
                );
            } else {
                return Err(anyhow::anyhow!("Invalid custom output type"));
            }
        } else {
            // This should not happen as we only return Custom output
            return Err(anyhow::anyhow!("Expected Custom output type"));
        }

        Ok(())
    }
}

/// Insert loan actions into the database.
pub async fn insert_loan_actions_to_db(
    db: Arc<DbPool>,
    actions: Vec<LoanActionModel>,
) -> Result<()> {
    let mut conn = db.get().await?;
    insert_into(bento_types::schema::loan_actions::table)
        .values(&actions)
        .execute(&mut conn)
        .await?;
    Ok(())
}

/// Insert loan details into the database.
pub async fn insert_loan_details_to_db(
    db: Arc<DbPool>,
    details: Vec<LoanDetailModel>,
) -> Result<()> {
    let mut conn = db.get().await?;
    insert_into(bento_types::schema::loan_details::table)
        .values(&details)
        .execute(&mut conn)
        .await?;
    Ok(())
}

pub fn convert_to_model(
    blocks: Vec<BlockAndEvents>,
    contract_address: &str,
) -> (Vec<LoanActionModel>, Vec<LoanDetailModel>) {
    // Your existing implementation...
    let mut loan_actions = Vec::new();
    let mut loan_details = Vec::new();
    for be in blocks {
        let events = be.events;
        for event in events {
            if event.contract_address.eq(&contract_address) {
                if let Some(action) = LoanActionType::from_event_index(event.event_index) {
                    handle_loan_action_event(&mut loan_actions, &event, action);
                } else if event.event_index == 1 {
                    handle_loan_detail_event(&event, &mut loan_details);
                }
            }
        }
    }
    (loan_actions, loan_details)
}

#[derive(Debug, thiserror::Error)]
#[error("CustomError: {msg}, {status}")]
pub struct CustomError {
    msg: String,
    status: u16,
}

impl CustomError {
    fn not_found(msg: String) -> Self {
        Self { msg, status: 404 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromSqlRow, DbEnum, Serialize, AsExpression)]
#[diesel(sql_type = SmallInt)]
#[diesel_enum(error_fn = CustomError::not_found)]
#[diesel_enum(error_type = CustomError)]
pub enum LoanActionType {
    LoanCreated,
    LoanCancelled,
    LoanPaid,
    LoanAccepted,
    LoanLiquidated,
}

impl LoanActionType {
    pub fn from_event_index(event_index: i32) -> Option<Self> {
        match event_index {
            2 => Some(Self::LoanCreated),
            3 => Some(Self::LoanCancelled),
            4 => Some(Self::LoanPaid),
            5 => Some(Self::LoanAccepted),
            6 => Some(Self::LoanLiquidated),
            _ => None,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Self::LoanCreated => "LoanCreated".to_string(),
            Self::LoanCancelled => "LoanCancelled".to_string(),
            Self::LoanPaid => "LoanPaid".to_string(),
            Self::LoanAccepted => "LoanAccepted".to_string(),
            Self::LoanLiquidated => "LoanLiquidated".to_string(),
        }
    }
}

fn handle_loan_action_event(
    models: &mut Vec<LoanActionModel>,
    event: &ContractEventByBlockHash,
    action: LoanActionType,
) {
    if event.fields.len() < 3 {
        tracing::warn!("Invalid event fields length: {}, skipping", event.fields.len());
    }

    match action {
        LoanActionType::LoanCreated => {
            models.push(LoanActionModel {
                loan_subcontract_id: event.fields[0].value.clone().to_string(),
                action_type: action,
                by: event.fields[2].value.clone().to_string(),
                timestamp: timestamp_millis_to_naive_datetime(
                    event.fields[3].value.as_str().unwrap().parse::<i64>().unwrap(),
                ),
                loan_id: Some(
                    BigDecimal::from_str(event.fields[1].value.as_str().unwrap()).unwrap(),
                ),
            });
        }
        _ => {
            models.push(LoanActionModel {
                loan_subcontract_id: event.fields[0].value.clone().to_string(),
                action_type: action,
                by: event.fields[1].value.clone().to_string(),
                timestamp: timestamp_millis_to_naive_datetime(
                    event.fields[2].value.as_str().unwrap().parse::<i64>().unwrap(),
                ),
                loan_id: None, // Other actions does not need this field
            });
        }
    }
}

fn handle_loan_detail_event(event: &ContractEventByBlockHash, models: &mut Vec<LoanDetailModel>) {
    if event.fields.len() != 8 {
        tracing::warn!("Invalid event fields length: {}, skipping", event.fields.len());
    }

    println!("{:?}", event.fields[3].value);
    models.push(LoanDetailModel {
        loan_subcontract_id: event.fields[0].value.clone().to_string(),
        lending_token_id: event.fields[1].value.clone().to_string(),
        collateral_token_id: event.fields[2].value.clone().to_string(),
        lending_amount: BigDecimal::from_f64(
            event.fields[3].value.as_str().unwrap().parse::<f64>().unwrap(),
        )
        .unwrap(),
        collateral_amount: BigDecimal::from_f64(
            event.fields[4].value.as_str().unwrap().parse::<f64>().unwrap(),
        )
        .unwrap(),
        interest_rate: BigDecimal::from_f64(
            event.fields[5].value.as_str().unwrap().parse::<f64>().unwrap(),
        )
        .unwrap(),
        duration: BigDecimal::from_f64(
            event.fields[6].value.as_str().unwrap().parse::<f64>().unwrap(),
        )
        .unwrap(),
        lender: event.fields[7].value.clone().to_string(),
    });
}
