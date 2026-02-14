//! Game session management for HTTP multiplayer.

use crate::games::tictactoe::{Game, Mark};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, instrument, warn};

/// Unique identifier for a game session.
pub type SessionId = String;

/// Unique identifier for a player.
pub type PlayerId = String;

/// Type of player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlayerType {
    /// Human player via TUI.
    Human,
    /// AI agent via MCP.
    Agent,
}

/// A player in a game session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    /// Player's unique ID.
    pub id: PlayerId,
    /// Player's name.
    pub name: String,
    /// Type of player.
    pub player_type: PlayerType,
    /// Which mark this player uses (X or O).
    pub mark: Mark,
}

/// A game session with two players.
#[derive(Debug, Clone)]
pub struct GameSession {
    /// Session ID.
    pub id: SessionId,
    /// The game state.
    pub game: Game,
    /// Player X.
    pub player_x: Option<Player>,
    /// Player O.
    pub player_o: Option<Player>,
}

impl GameSession {
    /// Creates a new game session.
    #[instrument]
    pub fn new(id: SessionId) -> Self {
        info!(session_id = %id, "Creating new game session");
        Self {
            id,
            game: Game::new(),
            player_x: None,
            player_o: None,
        }
    }

    /// Registers a player in the session.
    /// Returns the mark assigned to the player (X or O).
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn register_player(
        &mut self,
        id: PlayerId,
        name: String,
        player_type: PlayerType,
    ) -> Result<Mark, String> {
        // Assign to first available slot
        if self.player_x.is_none() {
            info!(player_id = %id, mark = "X", "Registering player as X");
            self.player_x = Some(Player {
                id,
                name,
                player_type,
                mark: Mark::X,
            });
            Ok(Mark::X)
        } else if self.player_o.is_none() {
            info!(player_id = %id, mark = "O", "Registering player as O");
            self.player_o = Some(Player {
                id,
                name,
                player_type,
                mark: Mark::O,
            });
            Ok(Mark::O)
        } else {
            warn!(player_id = %id, "Session already has 2 players");
            Err("Session already has 2 players".to_string())
        }
    }

    /// Gets the player with the given ID.
    #[instrument(skip(self), fields(session_id = %self.id, player_id))]
    pub fn get_player(&self, player_id: &str) -> Option<&Player> {
        if self.player_x.as_ref().map(|p| p.id.as_str()) == Some(player_id) {
            self.player_x.as_ref()
        } else if self.player_o.as_ref().map(|p| p.id.as_str()) == Some(player_id) {
            self.player_o.as_ref()
        } else {
            None
        }
    }

    /// Checks if it's the given player's turn.
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn is_players_turn(&self, player_id: &str) -> bool {
        let player = match self.get_player(player_id) {
            Some(p) => p,
            None => {
                debug!(player_id, "Player not found in session");
                return false;
            }
        };

        let current_mark = self.game.state().current_player();
        let is_turn = player.mark == current_mark;
        
        debug!(
            player_id,
            player_mark = ?player.mark,
            current_mark = ?current_mark,
            is_turn,
            "Checked if player's turn"
        );
        
        is_turn
    }

    /// Makes a move for the given player.
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn make_move(&mut self, player_id: &str, position: usize) -> Result<(), String> {
        // Validate player exists
        let player = self.get_player(player_id)
            .ok_or_else(|| {
                warn!(player_id, "Unknown player attempted move");
                "Unknown player".to_string()
            })?;

        // Validate it's their turn
        if !self.is_players_turn(player_id) {
            warn!(
                player_id,
                expected_mark = ?self.game.state().current_player(),
                player_mark = ?player.mark,
                "Player tried to move out of turn"
            );
            return Err(format!(
                "Not your turn. Waiting for player {:?}",
                self.game.state().current_player()
            ));
        }

        // Make the move
        self.game.make_move(position).map_err(|e| {
            warn!(player_id, position, error = %e, "Invalid move");
            format!("Invalid move: {}", e)
        })?;

        info!(
            player_id,
            position,
            status = ?self.game.state().status(),
            "Move completed successfully"
        );

        Ok(())
    }
}

/// Manages all game sessions.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<SessionId, GameSession>>>,
}

impl SessionManager {
    /// Creates a new session manager.
    #[instrument]
    pub fn new() -> Self {
        info!("Creating session manager");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Creates a new game session.
    #[instrument(skip(self))]
    pub fn create_session(&self, id: SessionId) -> Result<SessionId, String> {
        let mut sessions = self.sessions.lock().unwrap();
        
        if sessions.contains_key(&id) {
            warn!(session_id = %id, "Session already exists");
            return Err("Session already exists".to_string());
        }

        let session = GameSession::new(id.clone());
        sessions.insert(id.clone(), session);
        
        info!(session_id = %id, "Created new session");
        Ok(id)
    }

    /// Gets a session by ID.
    #[instrument(skip(self))]
    pub fn get_session(&self, id: &str) -> Option<GameSession> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(id).cloned();
        
        if session.is_none() {
            debug!(session_id = id, "Session not found");
        }
        
        session
    }

    /// Updates a session.
    #[instrument(skip(self, session), fields(session_id = %session.id))]
    pub fn update_session(&self, session: GameSession) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session.id.clone(), session);
        debug!("Session updated");
    }

    /// Lists all active session IDs.
    #[instrument(skip(self))]
    pub fn list_sessions(&self) -> Vec<SessionId> {
        let sessions = self.sessions.lock().unwrap();
        let ids: Vec<_> = sessions.keys().cloned().collect();
        info!(count = ids.len(), "Listed sessions");
        ids
    }

    /// Atomically registers a player in a session (thread-safe).
    /// Returns the assigned mark (X or O).
    #[instrument(skip(self))]
    pub fn register_player_atomic(
        &self,
        session_id: &str,
        player_id: String,
        name: String,
        player_type: PlayerType,
    ) -> Result<Mark, String> {
        let mut sessions = self.sessions.lock().unwrap();
        
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| "Session not found".to_string())?;
        
        // Register player while holding the lock
        session.register_player(player_id, name, player_type)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
