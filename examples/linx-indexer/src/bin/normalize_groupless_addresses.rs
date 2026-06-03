//! One-off cleanup: collapse Alephium groupless addresses to their bare form.
//!
//! Groupless addresses are emitted by the node with a group suffix (e.g. `ADDR:0`),
//! while the referral/API path stores the bare `ADDR`. This fragments a single user
//! across multiple `points_snapshots` rows. All point-earning activity for the affected
//! users is on group 0, so the fix is to strip the `:group` suffix everywhere and merge
//! the per-user histories into the bare identity.
//!
//! This is intentionally NOT a diesel migration: the transformation is irreversible
//! (the original suffix cannot be reconstructed), so it lives as a one-off binary.
//!
//! Usage:
//!   normalize_groupless_addresses           # dry run (rolls back, prints what would change)
//!   normalize_groupless_addresses apply      # commits the changes
//!
//! Always run the dry run first and eyeball the reported counts.

use bento_cli::get_database_url;
use bento_core::new_db_pool;
use diesel::sql_query;
use diesel_async::RunQueryDsl;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let apply = matches!(std::env::args().nth(1).as_deref(), Some("apply"));
    if apply {
        tracing::warn!("Running in APPLY mode — changes will be COMMITTED.");
    } else {
        tracing::info!("Running in DRY-RUN mode — changes will be ROLLED BACK. Pass `apply` to commit.");
    }

    let database_url = get_database_url().expect("DATABASE_URL must be set in environment");
    let db_pool = new_db_pool(&database_url, None).await?;
    let mut conn = db_pool.get().await?;

    sql_query("BEGIN").execute(&mut conn).await?;

    // Run the cleanup; on any error, roll back and propagate.
    let result = run_cleanup(&mut conn).await;
    if let Err(e) = result {
        sql_query("ROLLBACK").execute(&mut conn).await.ok();
        return Err(e);
    }

    if apply {
        sql_query("COMMIT").execute(&mut conn).await?;
        tracing::info!("Committed.");
    } else {
        sql_query("ROLLBACK").execute(&mut conn).await?;
        tracing::info!("Rolled back (dry run). Re-run with `apply` to commit.");
    }

    Ok(())
}

async fn run_cleanup(conn: &mut diesel_async::AsyncPgConnection) -> anyhow::Result<()> {
    // 1. Source tables: strip the `:group` suffix. These are keyed by `id` and have no
    //    pre-existing bare counterparts, so a blind update is safe (regular addresses
    //    contain no colon and are untouched by split_part).
    for (table, column) in [
        ("account_transactions", "address"),
        ("lending_events", "on_behalf"),
        ("lending_position_snapshots", "address"),
        ("linx_transactions", "user_address"),
    ] {
        let sql = format!(
            "UPDATE {table} SET {column} = split_part({column}, ':', 1) WHERE {column} LIKE '%:%'"
        );
        let n = sql_query(sql).execute(conn).await?;
        tracing::info!("{table}.{column}: stripped suffix on {n} row(s)");
    }

    // 2. points_snapshots: the suffixed `ADDR:0` rows carry the on-chain activity (base
    //    points + multiplier), while a bare `ADDR` row may already exist carrying signup
    //    bonus / referrer points. They collide on UNIQUE(address, snapshot_date).
    //
    //    2a. Where both exist for the same (snapshot_date, season_id), fold the suffixed
    //        row into the existing bare row (sum component points; take the activity row's
    //        multiplier since the bonus row has none).
    let merged = sql_query(
        r#"
        UPDATE points_snapshots b
        SET swap_points       = b.swap_points + s.swap_points,
            supply_points     = b.supply_points + s.supply_points,
            borrow_points     = b.borrow_points + s.borrow_points,
            base_points_total = b.base_points_total + s.base_points_total,
            multiplier_type   = COALESCE(s.multiplier_type, b.multiplier_type),
            multiplier_value  = CASE WHEN s.multiplier_value > 0 THEN s.multiplier_value ELSE b.multiplier_value END,
            multiplier_points = b.multiplier_points + s.multiplier_points,
            referral_points   = b.referral_points + s.referral_points,
            total_points      = b.total_points + s.total_points,
            total_volume_usd  = b.total_volume_usd + s.total_volume_usd
        FROM points_snapshots s
        WHERE s.address LIKE '%:%'
          AND b.address = split_part(s.address, ':', 1)
          AND b.snapshot_date = s.snapshot_date
          AND b.season_id = s.season_id
        "#,
    )
    .execute(conn)
    .await?;
    tracing::info!("points_snapshots: merged {merged} suffixed row(s) into existing bare rows");

    //    2b. Delete the suffixed rows that were just merged into a bare row.
    let deleted = sql_query(
        r#"
        DELETE FROM points_snapshots s
        WHERE s.address LIKE '%:%'
          AND EXISTS (
              SELECT 1 FROM points_snapshots b
              WHERE b.address = split_part(s.address, ':', 1)
                AND b.snapshot_date = s.snapshot_date
                AND b.season_id = s.season_id
          )
        "#,
    )
    .execute(conn)
    .await?;
    tracing::info!("points_snapshots: deleted {deleted} merged suffixed row(s)");

    //    2c. Relabel the remaining suffixed rows (no bare counterpart on that date/season,
    //        so this is now conflict-free) to the bare address.
    let relabeled = sql_query(
        "UPDATE points_snapshots SET address = split_part(address, ':', 1) WHERE address LIKE '%:%'",
    )
    .execute(conn)
    .await?;
    tracing::info!("points_snapshots: relabeled {relabeled} remaining suffixed row(s) to bare");

    // 3. Safety check: nothing suffixed should remain anywhere we touched.
    #[derive(diesel::QueryableByName)]
    struct Remaining {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        n: i64,
    }
    let remaining: Remaining = sql_query(
        r#"
        SELECT (
            (SELECT COUNT(*) FROM account_transactions WHERE address LIKE '%:%') +
            (SELECT COUNT(*) FROM lending_events WHERE on_behalf LIKE '%:%') +
            (SELECT COUNT(*) FROM lending_position_snapshots WHERE address LIKE '%:%') +
            (SELECT COUNT(*) FROM linx_transactions WHERE user_address LIKE '%:%') +
            (SELECT COUNT(*) FROM points_snapshots WHERE address LIKE '%:%')
        )::bigint AS n
        "#,
    )
    .get_result(conn)
    .await?;

    if remaining.n != 0 {
        anyhow::bail!("{} suffixed address(es) still remain after cleanup — aborting", remaining.n);
    }
    tracing::info!("Verified: no suffixed addresses remain.");

    Ok(())
}
