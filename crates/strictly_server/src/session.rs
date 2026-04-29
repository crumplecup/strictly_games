//! Game session management for HTTP multiplayer.

use crate::games::blackjack::session::BlackjackSession;
use crate::games::tictactoe::{AnyGame, GameSetup, Mark, Position};
use elicitation::DynamicToolRegistry;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use strictly_blackjack::{MultiRound, SeatResult};
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

// ── Shared blackjack table ────────────────────────────────────────────────────

/// One seat at the shared table: session ID, bankroll, bet, and a handle to
/// that connection's `DynamicToolRegistry` (so we can push notifications to it).
pub struct SeatEntry {
    /// Session ID for this seat (used to look up dialogue / state).
    pub session_id: String,
    /// Player's bankroll at the start of this hand.
    pub bankroll: u64,
    /// Bet placed — `None` until the player places their bet.
    pub bet: Option<u64>,
    /// Clone of this seat's `DynamicToolRegistry` (shares Arc state with the
    /// live `GameServer` registry, so calling `notify_tool_list_changed` on it
    /// fires the notification to that connection's peer).
    pub registry: DynamicToolRegistry,
}

impl std::fmt::Debug for SeatEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeatEntry")
            .field("session_id", &self.session_id)
            .field("bankroll", &self.bankroll)
            .field("bet", &self.bet)
            .finish()
    }
}

/// Phase of the shared blackjack table state machine.
pub enum SharedTablePhase {
    /// Waiting for players to join and place bets.
    Betting {
        /// Seats that have joined (0..num_seats).
        seats: Vec<SeatEntry>,
        /// Total seats expected before dealing.
        num_seats: usize,
    },
    /// All bets placed; all players act concurrently on their hands.
    PlayerTurns {
        /// Active multi-player round (shared shoe + all seat hands).
        round: MultiRound,
        /// Seats that have finished their turn (stand/bust/blackjack/surrender).
        /// When this set reaches `seat_registries.len()` the dealer plays.
        seats_done: HashSet<usize>,
        /// Cloned registries in seat order (for cross-seat notifications).
        seat_registries: Vec<DynamicToolRegistry>,
        /// Session IDs in seat order.
        seat_session_ids: Vec<String>,
        /// Bankrolls in seat order (for view tools).
        seat_bankrolls: Vec<u64>,
    },
    /// Hand finished; players vote to deal again.
    Finished {
        /// Settlement results in seat order.
        results: Vec<SeatResult>,
        /// Cloned registries in seat order.
        seat_registries: Vec<DynamicToolRegistry>,
        /// Session IDs in seat order.
        seat_session_ids: Vec<String>,
        /// Final bankrolls after settlement.
        seat_bankrolls: Vec<u64>,
        /// How many seats have voted to deal again.
        ready_count: usize,
        /// Total seat count (quorum target).
        num_seats: usize,
    },
}

impl std::fmt::Debug for SharedTablePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Betting { seats, num_seats } => f
                .debug_struct("Betting")
                .field("num_seats", num_seats)
                .field("seats_joined", &seats.len())
                .finish(),
            Self::PlayerTurns {
                seats_done,
                seat_session_ids,
                seat_registries,
                ..
            } => f
                .debug_struct("PlayerTurns")
                .field("seats_done", &seats_done.len())
                .field("seats_total", &seat_registries.len())
                .field("session_ids", seat_session_ids)
                .finish(),
            Self::Finished {
                ready_count,
                num_seats,
                seat_session_ids,
                ..
            } => f
                .debug_struct("Finished")
                .field("ready_count", ready_count)
                .field("num_seats", num_seats)
                .field("session_ids", seat_session_ids)
                .finish(),
        }
    }
}

/// Inner state of the shared table (behind the mutex).
#[derive(Debug)]
pub struct SharedTableState {
    /// Current phase.
    pub phase: SharedTablePhase,
}

/// Shared handle to the table state machine.
///
/// Cheaply cloneable — all clones refer to the same table.
pub type SharedTable = Arc<tokio::sync::Mutex<SharedTableState>>;

/// Create a new shared table in Betting phase.
pub fn new_shared_table(num_seats: usize) -> SharedTable {
    Arc::new(tokio::sync::Mutex::new(SharedTableState {
        phase: SharedTablePhase::Betting {
            seats: Vec::with_capacity(num_seats),
            num_seats,
        },
    }))
}

/// Serializable per-seat view of the shared table for REST observers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedTableSeatView {
    /// Current phase name: `"betting"`, `"waiting"`, `"player_turn"`, `"finished"`.
    pub phase: String,
    /// Player's bankroll (0 if not yet known).
    pub bankroll: u64,
    /// Human-readable state description for TUI display.
    pub description: String,
    /// True when the session has ended.
    pub is_terminal: bool,
}

impl SharedTableSeatView {
    /// Build a view from the shared table state for `seat_index`.
    pub fn from_table(state: &SharedTableState, seat_index: usize) -> Self {
        match &state.phase {
            SharedTablePhase::Betting { seats, num_seats } => {
                let bets_placed = seats.iter().filter(|s| s.bet.is_some()).count();
                let my_bet = seats.get(seat_index).and_then(|s| s.bet);
                if let Some(bet) = my_bet {
                    Self {
                        phase: "betting".to_string(),
                        bankroll: seats[seat_index].bankroll,
                        description: format!(
                            "💰 Bet placed: ${bet}\nWaiting for {}/{} players to bet...",
                            bets_placed, num_seats
                        ),
                        is_terminal: false,
                    }
                } else {
                    let bankroll = seats.get(seat_index).map(|s| s.bankroll).unwrap_or(0);
                    Self {
                        phase: "betting".to_string(),
                        bankroll,
                        description: format!(
                            "💰 Bankroll: ${bankroll}\n\nPlace your bet to begin."
                        ),
                        is_terminal: false,
                    }
                }
            }
            SharedTablePhase::PlayerTurns {
                round,
                seats_done,
                seat_bankrolls,
                ..
            } => {
                let bankroll = seat_bankrolls.get(seat_index).copied().unwrap_or(0);
                let seat = &round.seats[seat_index];
                let dealer_up = &round.dealer_hand.cards()[0];
                let done_count = seats_done.len();
                let total = seat_bankrolls.len();
                if seats_done.contains(&seat_index) {
                    Self {
                        phase: "waiting".to_string(),
                        bankroll,
                        description: format!(
                            "Your hand: {} — done\nWaiting for {}/{} players to finish...",
                            seat.hand.display(),
                            done_count,
                            total
                        ),
                        is_terminal: false,
                    }
                } else {
                    Self {
                        phase: "player_turn".to_string(),
                        bankroll,
                        description: format!(
                            "Your hand: {} (value: {})\nDealer shows: {}\n",
                            seat.hand.display(),
                            seat.hand.value().best(),
                            dealer_up
                        ),
                        is_terminal: false,
                    }
                }
            }
            SharedTablePhase::Finished {
                results,
                seat_bankrolls,
                ..
            } => {
                let bankroll = seat_bankrolls.get(seat_index).copied().unwrap_or(0);
                let desc = if let Some(r) = results.get(seat_index) {
                    format!(
                        "Hand: {} — {}\nBankroll: ${}\n\nDeal again or cash out.",
                        r.hand.display(),
                        r.outcome,
                        r.final_bankroll
                    )
                } else {
                    "Hand complete. Awaiting result...".to_string()
                };
                Self {
                    phase: "finished".to_string(),
                    bankroll,
                    description: desc,
                    is_terminal: false,
                }
            }
        }
    }
}

/// Manages all game sessions.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<SessionId, GameSession>>>,
    /// Server↔agent dialogue log, keyed by session ID.
    dialogue: Arc<Mutex<HashMap<SessionId, Vec<DialogueEntry>>>>,
    /// Shared blackjack phase state, keyed by session ID.
    ///
    /// The `BlackjackSession` Arc is cloned from the live `GameServer` instance,
    /// so the REST endpoint always reads the current phase state.
    blackjack_sessions: Arc<Mutex<HashMap<SessionId, BlackjackSession>>>,
    /// The shared blackjack table (one per server instance).
    ///
    /// All seats (human + agents) share this single table.
    shared_table: Arc<Mutex<Option<SharedTable>>>,
    /// Maps each seat's session_id → seat_index in the shared table.
    seat_indices: Arc<Mutex<HashMap<SessionId, usize>>>,
}

impl SessionManager {
    /// Creates a new session manager.
    #[instrument]
    pub fn new() -> Self {
        info!("Creating session manager");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            dialogue: Arc::new(Mutex::new(HashMap::new())),
            blackjack_sessions: Arc::new(Mutex::new(HashMap::new())),
            shared_table: Arc::new(Mutex::new(None)),
            seat_indices: Arc::new(Mutex::new(HashMap::new())),
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

    // ── Shared blackjack table ────────────────────────────────────────────────

    /// Initialises the shared table for `num_seats` players.
    ///
    /// Idempotent: if a table is already initialised, returns it unchanged.
    /// Returns the `SharedTable` handle.
    #[instrument(skip(self))]
    pub fn init_shared_table(&self, num_seats: usize) -> SharedTable {
        let mut guard = self.shared_table.lock().unwrap();
        if let Some(ref table) = *guard {
            debug!(num_seats, "Shared table already initialised");
            return table.clone();
        }
        info!(num_seats, "Initialising shared blackjack table");
        let table = new_shared_table(num_seats);
        *guard = Some(table.clone());
        table
    }

    /// Returns the shared table handle, if one has been initialised.
    #[instrument(skip(self))]
    pub fn get_shared_table(&self) -> Option<SharedTable> {
        self.shared_table.lock().unwrap().clone()
    }

    /// Records that `session_id` owns seat `seat_index`.
    #[instrument(skip(self))]
    pub fn register_seat_index(&self, session_id: SessionId, seat_index: usize) {
        let mut map = self.seat_indices.lock().unwrap();
        map.insert(session_id, seat_index);
        debug!(seat_index, "Registered seat index");
    }

    /// Returns the seat index for the given session, if registered.
    #[instrument(skip(self))]
    pub fn get_seat_index(&self, session_id: &str) -> Option<usize> {
        self.seat_indices.lock().unwrap().get(session_id).copied()
    }

    // ── Blackjack shared state (single-player legacy) ─────────────────────────

    /// Registers a live blackjack phase Arc for the given session.
    ///
    /// The Arc is shared with the `GameServer` instance, so readers always see
    /// the current phase without any extra update calls from the factories.
    #[instrument(skip(self, session))]
    pub fn store_blackjack_session(&self, session_id: SessionId, session: BlackjackSession) {
        let mut map = self.blackjack_sessions.lock().unwrap();
        map.insert(session_id.clone(), session);
        debug!(session_id = %session_id, "Stored blackjack session");
    }

    /// Returns the live blackjack phase Arc for the given session, if registered.
    #[instrument(skip(self))]
    pub fn get_blackjack_session(&self, session_id: &str) -> Option<BlackjackSession> {
        let map = self.blackjack_sessions.lock().unwrap();
        map.get(session_id).cloned()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
