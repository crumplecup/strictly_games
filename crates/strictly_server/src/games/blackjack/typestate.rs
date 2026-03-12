//! Phase-specific typestate structs for blackjack.
//!
//! Each phase is its own distinct type with phase-specific fields.
//! This encodes invariants at compile time.

use super::action::{ActionError, BasicAction, PlayerAction};
use super::contracts::{execute_action, validate_action};
use super::ledger::{BankrollLedger, BetDeducted};
use elicitation::contracts::Established;
use elicitation::{Elicit, Prompt, Select};
use strictly_blackjack::{Deck, Hand, Outcome};
use tracing::instrument;

// ─────────────────────────────────────────────────────────────
//  Setup Phase
// ─────────────────────────────────────────────────────────────

/// Game in setup phase - ready to start.
#[derive(Debug, Clone, Elicit)]
pub struct GameSetup {
    deck: Deck,
}

impl GameSetup {
    /// Creates a new game in setup phase with a shuffled deck.
    #[instrument]
    pub fn new() -> Self {
        Self {
            deck: Deck::new_shuffled(),
        }
    }

    /// Starts betting phase with initial bankroll (consumes setup, returns betting).
    #[instrument(skip(self))]
    pub fn start_betting(self, initial_bankroll: u64) -> GameBetting {
        GameBetting {
            deck: self.deck,
            bankroll: initial_bankroll,
        }
    }
}

impl Default for GameSetup {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────
//  Betting Phase
// ─────────────────────────────────────────────────────────────

/// Game in betting phase - player places bet.
#[derive(Debug, Clone, Elicit)]
pub struct GameBetting {
    deck: Deck,
    bankroll: u64,
}

impl GameBetting {
    /// Returns the current bankroll (before any bet is placed).
    pub fn bankroll(&self) -> u64 {
        self.bankroll
    }

    /// Places bet and deals initial cards (consumes betting, returns result).
    #[instrument(skip(self))]
    pub fn place_bet(self, bet: u64) -> Result<GameResult, ActionError> {
        // Debit establishes BetDeducted proof and validates bet in one step.
        let (ledger, bet_deducted) = BankrollLedger::debit(self.bankroll, bet)?;

        // Deal initial cards (2 to player, 2 to dealer)
        let mut deck = self.deck;
        let mut player_hand = Hand::new(Vec::new());
        let mut dealer_hand = Hand::new(Vec::new());

        for _ in 0..2 {
            if let Some(card) = deck.deal() {
                player_hand.add_card(card);
            } else {
                return Err(ActionError::DeckExhausted);
            }

            if let Some(card) = deck.deal() {
                dealer_hand.add_card(card);
            } else {
                return Err(ActionError::DeckExhausted);
            }
        }

        // Check for immediate blackjack — settle the ledger now.
        if player_hand.is_blackjack() {
            if dealer_hand.is_blackjack() {
                let (bankroll, _settled) = ledger.settle(Outcome::Push, bet_deducted);
                return Ok(GameResult::Finished(GameFinished {
                    player_hands: vec![player_hand],
                    dealer_hand,
                    bets: vec![bet],
                    outcomes: vec![Outcome::Push],
                    bankroll,
                }));
            } else {
                let (bankroll, _settled) = ledger.settle(Outcome::Blackjack, bet_deducted);
                return Ok(GameResult::Finished(GameFinished {
                    player_hands: vec![player_hand],
                    dealer_hand,
                    bets: vec![bet],
                    outcomes: vec![Outcome::Blackjack],
                    bankroll,
                }));
            }
        }

        // Dealer blackjack (player doesn't have it) — settle the ledger now.
        if dealer_hand.is_blackjack() {
            let (bankroll, _settled) = ledger.settle(Outcome::Loss, bet_deducted);
            return Ok(GameResult::Finished(GameFinished {
                player_hands: vec![player_hand],
                dealer_hand,
                bets: vec![bet],
                outcomes: vec![Outcome::Loss],
                bankroll,
            }));
        }

        // Normal game — carry the ledger + proof through to dealer resolution.
        Ok(GameResult::PlayerTurn(GamePlayerTurn {
            deck,
            player_hands: vec![player_hand],
            current_hand_index: 0,
            dealer_hand,
            bets: vec![bet],
            ledger,
            bet_deducted,
        }))
    }
}

// ─────────────────────────────────────────────────────────────
//  PlayerTurn Phase
// ─────────────────────────────────────────────────────────────

/// Game in player turn phase - player takes actions.
#[derive(Clone, Elicit)]
pub struct GamePlayerTurn {
    pub(super) deck: Deck,
    pub(super) player_hands: Vec<Hand>,
    pub(super) current_hand_index: usize,
    pub(super) dealer_hand: Hand,
    pub(super) bets: Vec<u64>,
    /// Financial ledger proving the bet was deducted exactly once.
    pub(super) ledger: BankrollLedger,
    /// Proof token that the bet has been debited; consumed at settlement.
    pub(super) bet_deducted: Established<BetDeducted>,
}

impl std::fmt::Debug for GamePlayerTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GamePlayerTurn")
            .field("player_hands", &self.player_hands)
            .field("current_hand_index", &self.current_hand_index)
            .field("dealer_hand", &self.dealer_hand)
            .field("bets", &self.bets)
            .field("ledger_balance", &self.ledger.post_bet_balance())
            .field("bet", &self.ledger.bet())
            .field("bet_deducted", &"<proof token>")
            .finish()
    }
}

impl GamePlayerTurn {
    /// Takes an action, consuming self and transitioning to next state.
    #[instrument(skip(self))]
    pub fn take_action(self, action: PlayerAction) -> Result<GameResult, ActionError> {
        // Validate action with contracts
        let proof = validate_action(&action, &self)?;

        // Execute with proof (zero-cost, enforced by type system)
        let mut game = self;
        execute_action(&action, &mut game, proof)?;

        // Check if current hand is complete (bust or stand)
        let current_hand = &game.player_hands[game.current_hand_index];
        let hand_complete = current_hand.is_bust() || action.action() == BasicAction::Stand;

        if hand_complete {
            game.advance_hand()
        } else {
            Ok(GameResult::PlayerTurn(game))
        }
    }

    /// Advances to next hand or transitions to dealer turn.
    #[instrument(skip(self))]
    fn advance_hand(mut self) -> Result<GameResult, ActionError> {
        self.current_hand_index += 1;

        if self.current_hand_index >= self.player_hands.len() {
            // All hands complete - move to dealer turn, carrying the ledger.
            Ok(GameResult::DealerTurn(GameDealerTurn {
                deck: self.deck,
                player_hands: self.player_hands,
                dealer_hand: self.dealer_hand,
                bets: self.bets,
                ledger: self.ledger,
                bet_deducted: self.bet_deducted,
            }))
        } else {
            // More hands to play
            Ok(GameResult::PlayerTurn(self))
        }
    }

    /// Returns all player hands.
    pub fn player_hands(&self) -> &[Hand] {
        &self.player_hands
    }

    /// Returns the dealer's hand (only showing first card in real game).
    pub fn dealer_hand(&self) -> &Hand {
        &self.dealer_hand
    }

    /// Returns which hand is being played (0-indexed).
    pub fn current_hand_index(&self) -> usize {
        self.current_hand_index
    }
}

// ─────────────────────────────────────────────────────────────
//  DealerTurn Phase
// ─────────────────────────────────────────────────────────────

/// Game in dealer turn phase - dealer plays by fixed rules.
#[derive(Clone, Elicit)]
pub struct GameDealerTurn {
    deck: Deck,
    player_hands: Vec<Hand>,
    dealer_hand: Hand,
    bets: Vec<u64>,
    /// Financial ledger threading the BetDeducted proof to settlement.
    ledger: BankrollLedger,
    /// Proof token: bet was deducted; required by BankrollLedger::settle.
    bet_deducted: Established<BetDeducted>,
}

impl std::fmt::Debug for GameDealerTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameDealerTurn")
            .field("player_hands", &self.player_hands)
            .field("dealer_hand", &self.dealer_hand)
            .field("bets", &self.bets)
            .field("ledger_balance", &self.ledger.post_bet_balance())
            .field("bet", &self.ledger.bet())
            .field("bet_deducted", &"<proof token>")
            .finish()
    }
}

impl GameDealerTurn {
    /// Plays dealer turn and resolves game (consumes dealer turn, returns finished).
    #[instrument(skip(self))]
    pub fn play_dealer_turn(mut self) -> GameFinished {
        // Dealer follows fixed rules: hit on 16 or less, stand on 17+
        // Use best value (soft if available)
        while self.dealer_hand.value().best() < 17 {
            if let Some(card) = self.deck.deal() {
                self.dealer_hand.add_card(card);
            } else {
                // Deck exhausted - dealer stands with current hand
                break;
            }
        }

        self.resolve()
    }

    /// Resolves outcomes and settles the bankroll via the proof-carrying ledger.
    ///
    /// Calls [`BankrollLedger::settle`] which consumes `Established<BetDeducted>`,
    /// proving that settlement occurs exactly once after a validated debit.
    #[instrument(skip(self))]
    fn resolve(self) -> GameFinished {
        let dealer_value = self.dealer_hand.value().best();
        let dealer_bust = self.dealer_hand.is_bust();

        let mut outcomes = Vec::with_capacity(self.player_hands.len());

        for hand in self.player_hands.iter() {
            let player_value = hand.value().best();
            let outcome = if hand.is_bust() {
                Outcome::Loss
            } else if dealer_bust || player_value > dealer_value {
                Outcome::Win
            } else if player_value < dealer_value {
                Outcome::Loss
            } else {
                Outcome::Push
            };
            outcomes.push(outcome);
        }

        // Settle via the ledger — consuming the BetDeducted proof guarantees
        // this path runs exactly once and uses gross_return arithmetic only.
        let primary_outcome = outcomes.first().copied().unwrap_or(Outcome::Loss);
        let (final_bankroll, _settled) = self.ledger.settle(primary_outcome, self.bet_deducted);

        GameFinished {
            player_hands: self.player_hands,
            dealer_hand: self.dealer_hand,
            bets: self.bets,
            outcomes,
            bankroll: final_bankroll,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Finished Phase
// ─────────────────────────────────────────────────────────────

/// Game finished - outcomes determined.
#[derive(Debug, Clone, Elicit)]
pub struct GameFinished {
    player_hands: Vec<Hand>,
    dealer_hand: Hand,
    bets: Vec<u64>,
    outcomes: Vec<Outcome>,
    bankroll: u64,
}

impl GameFinished {
    /// Returns player hands.
    pub fn player_hands(&self) -> &[Hand] {
        &self.player_hands
    }

    /// Returns dealer hand.
    pub fn dealer_hand(&self) -> &Hand {
        &self.dealer_hand
    }

    /// Returns bets for each hand.
    pub fn bets(&self) -> &[u64] {
        &self.bets
    }

    /// Returns outcomes for each hand.
    pub fn outcomes(&self) -> &[Outcome] {
        &self.outcomes
    }

    /// Returns final bankroll.
    pub fn bankroll(&self) -> u64 {
        self.bankroll
    }
}

// ─────────────────────────────────────────────────────────────
//  Result Type
// ─────────────────────────────────────────────────────────────

/// Result of a game transition.
#[derive(Debug, Elicit)]
pub enum GameResult {
    /// Game in player turn phase.
    PlayerTurn(GamePlayerTurn),
    /// Game in dealer turn phase.
    DealerTurn(GameDealerTurn),
    /// Game finished.
    Finished(GameFinished),
}
