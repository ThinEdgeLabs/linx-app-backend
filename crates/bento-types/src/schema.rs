// @generated automatically by Diesel CLI.

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
    events (id) {
        id -> Text,
        tx_id -> Text,
        contract_address -> Text,
        event_index -> Int4,
        fields -> Jsonb,
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

diesel::allow_tables_to_appear_in_same_query!(blocks, events, processor_status, transactions);
