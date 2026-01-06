// @generated automatically by Diesel CLI.

diesel::table! {
    account_transactions (id) {
        id -> Int8,
        address -> Text,
        tx_type -> Text,
        from_group -> Int2,
        to_group -> Int2,
        block_height -> Int8,
        tx_id -> Text,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    blocks (hash) {
        hash -> Text,
        timestamp -> Timestamp,
        chain_from -> Int8,
        chain_to -> Int8,
        height -> Int8,
        nonce -> Text,
        version -> Text,
        dep_state_hash -> Text,
        txs_hash -> Text,
        tx_number -> Int8,
        target -> Text,
        ghost_uncles -> Jsonb,
        main_chain -> Bool,
        deps -> Array<Nullable<Text>>,
    }
}

diesel::table! {
    contract_calls (id) {
        id -> Int8,
        account_transaction_id -> Int8,
        contract_address -> Text,
        tx_id -> Text,
    }
}

diesel::table! {
    events (id) {
        id -> Text,
        tx_id -> Text,
        contract_address -> Text,
        event_index -> Int4,
        fields -> Jsonb,
    }
}

diesel::table! {
    lending_events (id) {
        id -> Int8,
        market_id -> Text,
        event_type -> Text,
        token_id -> Text,
        on_behalf -> Text,
        amount -> Numeric,
        shares -> Numeric,
        transaction_id -> Text,
        event_index -> Int4,
        block_time -> Timestamp,
        created_at -> Timestamp,
        fields -> Jsonb,
    }
}

diesel::table! {
    lending_markets (id) {
        id -> Text,
        market_contract_id -> Text,
        collateral_token -> Text,
        loan_token -> Text,
        oracle -> Text,
        irm -> Text,
        ltv -> Numeric,
        created_at -> Timestamp,
    }
}

diesel::table! {
    lending_position_snapshots (id) {
        id -> Int8,
        address -> Text,
        market_id -> Text,
        supply_amount -> Numeric,
        supply_amount_usd -> Numeric,
        timestamp -> Timestamp,
        borrow_amount -> Numeric,
        borrow_amount_usd -> Numeric,
    }
}

diesel::table! {
    loan_actions (id) {
        id -> Int4,
        loan_subcontract_id -> Varchar,
        loan_id -> Nullable<Numeric>,
        by -> Varchar,
        timestamp -> Timestamp,
        action_type -> Int2,
    }
}

diesel::table! {
    loan_details (id) {
        id -> Int4,
        loan_subcontract_id -> Varchar,
        lending_token_id -> Varchar,
        collateral_token_id -> Varchar,
        lending_amount -> Numeric,
        collateral_amount -> Numeric,
        interest_rate -> Numeric,
        duration -> Numeric,
        lender -> Varchar,
    }
}

diesel::table! {
    market_state_snapshots (id) {
        id -> Int4,
        market_id -> Text,
        total_supply_assets -> Numeric,
        total_supply_shares -> Numeric,
        total_borrow_assets -> Numeric,
        total_borrow_shares -> Numeric,
        interest_rate -> Nullable<Numeric>,
        snapshot_timestamp -> Timestamp,
        created_at -> Timestamp,
    }
}

diesel::table! {
    points_config (id) {
        id -> Int4,
        action_type -> Text,
        points_per_usd -> Nullable<Numeric>,
        points_per_usd_per_day -> Nullable<Numeric>,
        is_active -> Bool,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    points_multipliers (id) {
        id -> Int4,
        multiplier_type -> Text,
        threshold_value -> Numeric,
        multiplier -> Numeric,
        is_active -> Bool,
        created_at -> Timestamp,
    }
}

diesel::table! {
    points_seasons (id) {
        id -> Int4,
        season_number -> Int4,
        start_date -> Date,
        end_date -> Date,
        max_tokens_distribution -> Numeric,
        is_active -> Bool,
        created_at -> Timestamp,
    }
}

diesel::table! {
    points_snapshots (id) {
        id -> Int4,
        address -> Text,
        snapshot_date -> Date,
        swap_points -> Int4,
        supply_points -> Int4,
        borrow_points -> Int4,
        base_points_total -> Int4,
        multiplier_type -> Nullable<Text>,
        multiplier_value -> Numeric,
        multiplier_points -> Int4,
        referral_points -> Int4,
        total_points -> Int4,
        total_volume_usd -> Numeric,
        created_at -> Timestamp,
        season_id -> Int4,
    }
}

diesel::table! {
    points_transactions (id) {
        id -> Int4,
        address -> Text,
        action_type -> Text,
        transaction_id -> Nullable<Text>,
        amount_usd -> Numeric,
        points_earned -> Int4,
        created_at -> Timestamp,
        snapshot_date -> Date,
        season_id -> Int4,
    }
}

diesel::table! {
    pools (id) {
        id -> Int8,
        address -> Text,
        token_a -> Text,
        token_b -> Text,
        factory_address -> Text,
    }
}

diesel::table! {
    processor_status (processor) {
        #[max_length = 50]
        processor -> Varchar,
        last_timestamp -> Int8,
    }
}

diesel::table! {
    referral_codes (id) {
        id -> Int4,
        code -> Text,
        owner_address -> Text,
        created_at -> Timestamp,
    }
}

diesel::table! {
    swaps (id) {
        id -> Int8,
        account_transaction_id -> Int8,
        token_in -> Text,
        token_out -> Text,
        amount_in -> Numeric,
        amount_out -> Numeric,
        pool_address -> Text,
        tx_id -> Text,
    }
}

diesel::table! {
    transactions (tx_hash) {
        tx_hash -> Text,
        unsigned -> Jsonb,
        script_execution_ok -> Bool,
        contract_inputs -> Jsonb,
        generated_outputs -> Jsonb,
        input_signatures -> Array<Nullable<Text>>,
        script_signatures -> Array<Nullable<Text>>,
        created_at -> Nullable<Timestamptz>,
        updated_at -> Nullable<Timestamptz>,
        main_chain -> Bool,
        block_hash -> Nullable<Text>,
    }
}

diesel::table! {
    transfers (id) {
        id -> Int8,
        account_transaction_id -> Int8,
        token_id -> Text,
        from_address -> Text,
        to_address -> Text,
        amount -> Numeric,
        tx_id -> Text,
    }
}

diesel::table! {
    user_referrals (id) {
        id -> Int4,
        user_address -> Text,
        referred_by_address -> Text,
        created_at -> Timestamp,
    }
}

diesel::joinable!(contract_calls -> account_transactions (account_transaction_id));
diesel::joinable!(points_snapshots -> points_seasons (season_id));
diesel::joinable!(points_transactions -> points_seasons (season_id));
diesel::joinable!(swaps -> account_transactions (account_transaction_id));
diesel::joinable!(transfers -> account_transactions (account_transaction_id));

diesel::allow_tables_to_appear_in_same_query!(
    account_transactions,
    blocks,
    contract_calls,
    events,
    lending_events,
    lending_markets,
    lending_position_snapshots,
    loan_actions,
    loan_details,
    market_state_snapshots,
    points_config,
    points_multipliers,
    points_seasons,
    points_snapshots,
    points_transactions,
    pools,
    processor_status,
    referral_codes,
    swaps,
    transactions,
    transfers,
    user_referrals,
);
