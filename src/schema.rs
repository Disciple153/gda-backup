// @generated automatically by Diesel CLI.

diesel::table! {
    glacier_state (file_path) {
        file_path -> Text,
        modified -> Timestamp,
        uploaded -> Nullable<Timestamp>,
        pending_delete -> Bool,
    }
}

diesel::table! {
    local_state (file_path) {
        file_path -> Text,
        modified -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    glacier_state,
    local_state,
);
