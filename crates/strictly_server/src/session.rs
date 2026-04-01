//! Game session management for HTTP multiplayer.

use crate::games::tictactoe::{AnyGame, GameSetup, Mark, Position};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::{debug, info, instrument, warn};

/// Unique identifier for a game session.
pub type SessionId = String;

/// Tracks how many explore vs play actions an agent has taken.
///
/// Surfaced in the typestate graph and story pane so the human player
/// can tell at a glance whether the agent is exploring productively
/// or stuck in an explore whirlpool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExploreStats {
    /// Cumulative explore actions this game.
    pub total_explores: usize,
    /// Cumulative commit (play) actions this game.
    pub total_plays: usize,
    /// Explore count in the current turn (resets each commit).
    pub turn_explores: usize,
}

impl ExploreStats {
    /// Records an explore action.
    pub fn record_explore(&mut self) {
        self.total_explores += 1;
        self.turn_explores += 1;
    }

    /// Records a commit (play) action, resetting the per-turn counter.
    pub fn record_play(&mut self) {
        self.total_plays += 1;
        self.turn_explores = 0;
    }

    /// Formats a compact status line for the story pane or callout.
    ///
    /// Example: `"🔍 3 explores / 2 plays (1 this turn)"`
    pub fn status_line(&self) -> String {
        if self.total_explores == 0 && self.total_plays == 0 {
            return String::new();
        }
        format!(
            "🔍 {} explores / {} plays ({} this turn)",
            self.total_explores, self.total_plays, self.turn_explores
        )
    }
}

/// A single line of server↔agent dialogue.
///
/// Recorded during the explore/play loop so the TUI can render
/// the exchange between the game server and the AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueEntry {
    /// Who sent the message: `"Server"` or `"Agent"`.
    pub role: String,
    /// The message text.
    pub text: String,
}

impl DialogueEntry {
    /// Creates a server-originated dialogue entry.
    pub fn server(text: impl Into<String>) -> Self {
        Self {
            role: "Server".to_string(),
            text: text.into(),
        }
    }

    /// Creates an agent-originated dialogue entry.
    pub fn agent(text: impl Into<String>) -> Self {
        Self {
            role: "Agent".to_string(),
            text: text.into(),
        }
    }
}

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
    /// Linked user profile ID (set after lobby profile selection).
    pub user_id: Option<i32>,
}

/// A game session with two players.
#[derive(Debug, Clone)]
pub struct GameSession {
    /// Session ID.
    pub id: SessionId,
    /// The game state (in any phase).
    pub game: AnyGame,
    /// Player X.
    pub player_x: Option<Player>,
    /// Player O.
    pub player_o: Option<Player>,
    /// Cancellation token for passive-Affirm escape hatch.
    /// When true, the game loop should gracefully exit.
    pub cancellation_tx: watch::Sender<bool>,
    /// Receiver for cancellation signal.
    cancellation_rx: watch::Receiver<bool>,
    /// Agent explore/play tracking stats.
    pub explore_stats: ExploreStats,
}

impl GameSession {
    /// Creates a new game session.
    #[instrument]
    pub fn new(id: SessionId) -> Self {
        info!(session_id = %id, "Creating new game session");
        let (cancellation_tx, cancellation_rx) = watch::channel(false);
        Self {
            id,
            game: GameSetup::new()
                .start(crate::games::tictactoe::Player::X)
                .into(),
            player_x: None,
            player_o: None,
            cancellation_tx,
            cancellation_rx,
            explore_stats: ExploreStats::default(),
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
        // Check if player is already registered (idempotent)
        if let Some(player) = self.get_player(&id) {
            info!(player_id = %id, mark = ?player.mark, "Player already registered, returning existing mark");
            return Ok(player.mark);
        }

        // Assign to first available slot
        if self.player_x.is_none() {
            info!(player_id = %id, mark = "X", "Registering player as X");
            self.player_x = Some(Player {
                id,
                name,
                player_type,
                mark: Mark::X,
                user_id: None,
            });
            Ok(Mark::X)
        } else if self.player_o.is_none() {
            info!(player_id = %id, mark = "O", "Registering player as O");
            self.player_o = Some(Player {
                id,
                name,
                player_type,
                mark: Mark::O,
                user_id: None,
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

        let Some(current_mark) = self.game.to_move() else {
            // Game is over
            return false;
        };
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
    pub fn make_move(&mut self, player_id: &str, position: Position) -> Result<(), String> {
        // Validate player exists
        let player = self.get_player(player_id).ok_or_else(|| {
            warn!(player_id, "Unknown player attempted move");
            "Unknown player".to_string()
        })?;

        // Validate it's their turn
        if !self.is_players_turn(player_id) {
            let expected = self
                .game
                .to_move()
                .ok_or_else(|| "Game is over".to_string())?;
            warn!(
                player_id,
                expected_mark = ?expected,
                player_mark = ?player.mark,
                "Player tried to move out of turn"
            );
            return Err(format!("Not your turn. Waiting for player {:?}", expected));
        }

        // Construct Move action (first-class domain event)
        let action = crate::games::tictactoe::Move::new(player.mark, position);
        debug!(action = %action, "Applying Move action");

        // Make the move (consuming transition via wrapper)
        let old_game = std::mem::replace(
            &mut self.game,
            GameSetup::new()
                .start(crate::games::tictactoe::Player::X)
                .into(),
        );
        self.game = old_game.make_move_action(action).map_err(|e| {
            warn!(player_id, action = %action, error = %e, "Invalid move");
            format!("Invalid move: {}", e)
        })?;

        info!(
            player_id,
            position = ?position,
            status = %self.game.status_string(),
            "Move completed successfully"
        );

        Ok(())
    }

    /// Passive-Affirm escape hatch: Check if game should continue.
    ///
    /// This is the building block for control flow - returns true if the game
    /// loop should continue, false if cancelled (user pressed 'q').
    ///
    /// This is a **passive** affirm - no user prompt, just a flag check.
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn affirm_continue(&self) -> bool {
        let cancelled = *self.cancellation_rx.borrow();
        if cancelled {
            info!("Game loop cancelled via escape hatch");
        }
        !cancelled
    }

    /// Request cancellation of the game loop (called when user presses 'q').
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn request_cancel(&self) {
        info!("Requesting game loop cancellation");
        let _ = self.cancellation_tx.send(true);
    }

    /// Reset cancellation flag (e.g., when starting a new game).
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn reset_cancel(&self) {
        debug!("Resetting cancellation flag");
        let _ = self.cancellation_tx.send(false);
    }

    /// Links a session player to a user profile by database ID.
    #[instrument(skip(self), fields(session_id = %self.id))]
    pub fn set_player_user_id(&mut self, player_id: &str, user_id: i32) -> Result<(), String> {
        debug!(player_id = %player_id, user_id = %user_id, "Linking player to user profile");

        if let Some(ref mut player) = self.player_x
            && player.id == player_id
        {
            player.user_id = Some(user_id);
            info!(player_id = %player_id, user_id = %user_id, "Player X linked to profile");
            return Ok(());
        }
        if let Some(ref mut player) = self.player_o
            && player.id == player_id
        {
            player.user_id = Some(user_id);
            info!(player_id = %player_id, user_id = %user_id, "Player O linked to profile");
            return Ok(());
        }

        warn!(player_id = %player_id, "Player not found for profile linking");
        Err(format!("Player not found: {}", player_id))
    }
}

/// Manages all game sessions.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<SessionId, GameSession>>>,
    /// Server↔agent dialogue log, keyed by session ID.
    dialogue: Arc<Mutex<HashMap<SessionId, Vec<DialogueEntry>>>>,
}

impl SessionManager {
    /// Creates a new session manager.
    #[instrument]
    pub fn new() -> Self {
        info!("Creating session manager");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            dialogue: Arc::new(Mutex::new(HashMap::new())),
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

    /// Atomically updates game state without overwriting player registrations.
    #[instrument(skip(self, game))]
    pub fn update_game_atomic(&self, session_id: &str, game: AnyGame) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| "Session not found".to_string())?;

        session.game = game;
        debug!("Game state updated atomically");
        Ok(())
    }

    /// Restarts game in session (keeps players registered).
    #[instrument(skip(self))]
    pub fn restart_game(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| "Session not found".to_string())?;

        session.game = GameSetup::new()
            .start(crate::games::tictactoe::Player::X)
            .into();
        session.explore_stats = ExploreStats::default();
        // Release session lock before taking dialogue lock.
        drop(sessions);
        self.clear_dialogue(session_id);
        info!("Game restarted with same players");
        Ok(())
    }

    /// Records an agent explore action on a session.
    #[instrument(skip(self))]
    pub fn record_explore(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            session.explore_stats.record_explore();
            debug!(
                total_explores = session.explore_stats.total_explores,
                turn_explores = session.explore_stats.turn_explores,
                "Recorded explore action"
            );
        }
    }

    /// Records an agent commit (play) action on a session.
    #[instrument(skip(self))]
    pub fn record_play(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            session.explore_stats.record_play();
            debug!(
                total_plays = session.explore_stats.total_plays,
                "Recorded play action"
            );
        }
    }

    /// Appends a dialogue entry for the given session.
    #[instrument(skip(self, entry), fields(role = %entry.role))]
    pub fn push_dialogue(&self, session_id: &str, entry: DialogueEntry) {
        let mut dialogue = self.dialogue.lock().unwrap();
        dialogue
            .entry(session_id.to_string())
            .or_default()
            .push(entry);
    }

    /// Returns all dialogue entries for a session.
    #[instrument(skip(self))]
    pub fn get_dialogue(&self, session_id: &str) -> Vec<DialogueEntry> {
        let dialogue = self.dialogue.lock().unwrap();
        dialogue.get(session_id).cloned().unwrap_or_default()
    }

    /// Clears the dialogue log for a session (called on restart).
    #[instrument(skip(self))]
    pub fn clear_dialogue(&self, session_id: &str) {
        let mut dialogue = self.dialogue.lock().unwrap();
        dialogue.remove(session_id);
        debug!("Cleared dialogue log");
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
