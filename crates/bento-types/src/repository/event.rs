use std::sync::Arc;
use std::time::Duration;

use crate::{models::event::EventModel, DbPool};
use anyhow::Result;
use diesel::insert_into;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

/// Insert events into the database.
pub async fn insert_events_to_db(db: Arc<DbPool>, events: Vec<EventModel>) -> Result<()> {
    // Log the start with event count
    tracing::debug!("Starting DB insertion process for {} events", events.len());

    // 1. Acquire connection with timeout
    tracing::debug!("Attempting to acquire DB connection");
    let conn_future = db.get();
    let mut conn = match tokio::time::timeout(Duration::from_secs(5), conn_future).await {
        Ok(result) => {
            let conn = result?;
            tracing::info!("Successfully acquired DB connection");
            conn
        }
        Err(_) => {
            tracing::error!("Timed out waiting for database connection");
            return Err(anyhow::anyhow!("Timed out waiting for database connection"));
        }
    };

    // Now try the full batch
    tracing::debug!("Executing full insert query for {} events", events.len());
    match tokio::time::timeout(
        Duration::from_secs(30), // Increased timeout for larger batches
        insert_into(crate::schema::events::table).values(&events).on_conflict_do_nothing().execute(&mut conn),
    )
    .await
    {
        Ok(result) => match result {
            Ok(rows) => {
                tracing::info!("Successfully inserted {} events", rows);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Database insert error: {}", e);
                Err(anyhow::anyhow!("Database insert error: {}", e))
            }
        },
        Err(_) => {
            tracing::error!("Database insert operation timed out");
            Err(anyhow::anyhow!("Database insert operation timed out"))
        }
    }
}

pub async fn get_events(db: Arc<DbPool>, limit: i64, offset: i64) -> Result<Vec<EventModel>> {
    use crate::schema::events::dsl::*;

    let mut conn = db.get().await?;

    let event_models: Vec<EventModel> =
        events.limit(limit).offset(offset).select(EventModel::as_select()).load(&mut conn).await?;

    Ok(event_models)
}

pub async fn get_events_by_contract(
    db: Arc<DbPool>,
    contract_address_value: String,
    limit: i64,
    offset: i64,
) -> Result<Vec<EventModel>> {
    use crate::schema::events::dsl::*;

    let mut conn = db.get().await?;

    let event_models: Vec<EventModel> = events
        .filter(contract_address.eq(contract_address_value))
        .limit(limit)
        .offset(offset)
        .select(EventModel::as_select())
        .load(&mut conn)
        .await?;

    Ok(event_models)
}

pub async fn get_events_by_tx(
    db: Arc<DbPool>,
    tx_id_value: String,
    limit: i64,
    offset: i64,
) -> Result<Vec<EventModel>> {
    use crate::schema::events::dsl::*;

    let mut conn = db.get().await?;

    let event_models: Vec<EventModel> = events
        .filter(tx_id.eq(tx_id_value))
        .limit(limit)
        .offset(offset)
        .select(EventModel::as_select())
        .load(&mut conn)
        .await?;

    Ok(event_models)
}
