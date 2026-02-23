CREATE TABLE game_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    user_id INTEGER NOT NULL,
    opponent_name TEXT NOT NULL,
    game_type TEXT NOT NULL,
    outcome TEXT NOT NULL,
    played_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    moves_count INTEGER NOT NULL,
    session_id TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_game_stats_user_id ON game_stats(user_id);
CREATE INDEX idx_game_stats_played_at ON game_stats(played_at DESC);
CREATE INDEX idx_game_stats_game_type ON game_stats(game_type);
