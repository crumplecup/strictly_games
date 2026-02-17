// @generated automatically by Diesel CLI.

diesel::table! {
    users (id) {
        id -> Integer,
        display_name -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

diesel::table! {
    game_stats (id) {
        id -> Integer,
        user_id -> Integer,
        opponent_name -> Text,
        game_type -> Text,
        outcome -> Text,
        played_at -> Timestamp,
        moves_count -> Integer,
        session_id -> Text,
    }
}

diesel::joinable!(game_stats -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(game_stats, users,);
