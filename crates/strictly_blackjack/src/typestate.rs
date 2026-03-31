//! Phase-specific typestate structs for blackjack.
//!
//! Each phase is its own distinct type with phase-specific fields.
//! This encodes invariants at compile time — you cannot call `execute_dealer_turn`
//! without first having `Established<PlayerTurnComplete>`, etc.

use elicitation::contracts::Established;
use elicitation::{Elicit, Generator, Prompt, Select};
use tracing::instrument;

use crate::{
    ActionError, BankrollLedger, BasicAction, BetDeducted, Hand, Outcome, PayoutSettled,
    PlayerAction, Shoe, execute_action, validate_action,
};
use crate::{MAX_HAND_CARDS, MAX_PLAYER_HANDS};

// ─────────────────────────────────────────────────────────────
//  Setup Phase
// ─────────────────────────────────────────────────────────────

/// Game in setup phase — ready to start.
#[derive(Debug, Clone, Default, Elicit)]
pub struct GameSetup {
    shoe: Shoe,
}

impl GameSetup {
    /// Creates a new game in setup phase with a shuffled single-deck shoe.
    #[cfg(feature = "shuffle")]
    #[instrument]
    pub fn new(seed: u64) -> Self {
        Self {
            shoe: Shoe::new(seed, 1),
        }
    }

    /// Creates a new game from a pre-built shoe (for testing / formal verification).
    pub fn with_shoe(shoe: Shoe) -> Self {
        Self { shoe }
    }

    /// Starts betting phase with initial bankroll (consumes setup, returns betting).
    #[instrument(skip(self))]
    pub fn start_betting(self, initial_bankroll: u64) -> GameBetting {
        GameBetting {
            shoe: self.shoe,
            bankroll: initial_bankroll,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Betting Phase
// ─────────────────────────────────────────────────────────────

/// Game in betting phase — player places bet.
#[derive(Debug, Clone, Elicit)]
pub struct GameBetting {
    shoe: Shoe,
    bankroll: u64,
}

impl GameBetting {
    /// Creates a new betting phase with the given shoe and bankroll.
    ///
    /// Primarily useful for formal verification harnesses where a deterministic
    /// shoe is required.
    pub fn new(shoe: Shoe, bankroll: u64) -> Self {
        Self { shoe, bankroll }
    }

    /// Returns the current bankroll (before any bet is placed).
    pub fn bankroll(&self) -> u64 {
        self.bankroll
    }

    /// Places bet and deals initial cards (consumes betting, returns result).
    #[instrument(skip(self))]
    pub fn place_bet(self, bet: u64) -> Result<GameResult, ActionError> {
        let (ledger, bet_deducted) = BankrollLedger::debit(self.bankroll, bet)?;

        let shoe = self.shoe;
        let mut player_hand = Hand::empty();
        let mut dealer_hand = Hand::empty();

        for _ in 0..2 {
            if let Some(card) = shoe.generate() {
                player_hand.add_card(card);
            } else {
                return Err(ActionError::DeckExhausted);
            }
            if let Some(card) = shoe.generate() {
                dealer_hand.add_card(card);
            } else {
                return Err(ActionError::DeckExhausted);
            }
        }

        // Check for immediate blackjack — settle the ledger now.
        if player_hand.is_blackjack() {
            let mut player_hands = [Hand::empty(); MAX_PLAYER_HANDS];
            player_hands[0] = player_hand;
            let mut bets_arr = [0u64; MAX_PLAYER_HANDS];
            bets_arr[0] = bet;

            if dealer_hand.is_blackjack() {
                let (bankroll, settled) = ledger.settle(Outcome::Push, bet_deducted);
                let mut outcomes = [Outcome::default(); MAX_PLAYER_HANDS];
                outcomes[0] = Outcome::Push;
                return Ok(GameResult::Finished(
                    GameFinished {
                        player_hands,
                        num_hands: 1,
                        dealer_hand,
                        bets: bets_arr,
                        outcomes,
                        bankroll,
                    },
                    settled,
                ));
            } else {
                let (bankroll, settled) = ledger.settle(Outcome::Blackjack, bet_deducted);
                let mut outcomes = [Outcome::default(); MAX_PLAYER_HANDS];
                outcomes[0] = Outcome::Blackjack;
                return Ok(GameResult::Finished(
                    GameFinished {
                        player_hands,
                        num_hands: 1,
                        dealer_hand,
                        bets: bets_arr,
                        outcomes,
                        bankroll,
                    },
                    settled,
                ));
            }
        }

        // Dealer blackjack (player doesn't have it) — settle immediately.
        if dealer_hand.is_blackjack() {
            let (bankroll, settled) = ledger.settle(Outcome::Loss, bet_deducted);
            let mut player_hands = [Hand::empty(); MAX_PLAYER_HANDS];
            player_hands[0] = player_hand;
            let mut bets_arr = [0u64; MAX_PLAYER_HANDS];
            bets_arr[0] = bet;
            let mut outcomes = [Outcome::default(); MAX_PLAYER_HANDS];
            outcomes[0] = Outcome::Loss;
            return Ok(GameResult::Finished(
                GameFinished {
                    player_hands,
                    num_hands: 1,
                    dealer_hand,
                    bets: bets_arr,
                    outcomes,
                    bankroll,
                },
                settled,
            ));
        }

        // Normal game — carry the ledger + proof through to dealer resolution.
        let mut player_hands = [Hand::empty(); MAX_PLAYER_HANDS];
        player_hands[0] = player_hand;
        let mut bets_arr = [0u64; MAX_PLAYER_HANDS];
        bets_arr[0] = bet;
        Ok(GameResult::PlayerTurn(GamePlayerTurn {
            shoe,
            player_hands,
            num_hands: 1,
            current_hand_index: 0,
            dealer_hand,
            bets: bets_arr,
            ledger,
            bet_deducted,
        }))
    }
}

// ─────────────────────────────────────────────────────────────
//  PlayerTurn Phase
// ─────────────────────────────────────────────────────────────

/// Game in player turn phase — player takes actions.
#[derive(Clone, Elicit)]
pub struct GamePlayerTurn {
    pub(crate) shoe: Shoe,
    pub(crate) player_hands: [Hand; MAX_PLAYER_HANDS],
    pub(crate) num_hands: usize,
    pub(crate) current_hand_index: usize,
    pub(crate) dealer_hand: Hand,
    pub(crate) bets: [u64; MAX_PLAYER_HANDS],
    /// Financial ledger proving the bet was deducted exactly once.
    pub(crate) ledger: BankrollLedger,
    /// Proof token that the bet has been debited; consumed at settlement.
    pub(crate) bet_deducted: Established<BetDeducted>,
}

impl std::fmt::Debug for GamePlayerTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active_hands = &self.player_hands[..self.num_hands];
        let active_bets = &self.bets[..self.num_hands];
        f.debug_struct("GamePlayerTurn")
            .field("player_hands", &active_hands)
            .field("current_hand_index", &self.current_hand_index)
            .field("dealer_hand", &self.dealer_hand)
            .field("bets", &active_bets)
            .field("ledger_balance", &self.ledger.post_bet_balance())
            .field("bet", &self.ledger.bet())
            .field("bet_deducted", &"<proof token>")
            .finish()
    }
}

impl GamePlayerTurn {
    /// Takes an action targeting the current hand — convenience wrapper that
    /// eliminates manual hand-index management.
    ///
    /// Equivalent to `take_action(PlayerAction::new(action, self.current_hand_index()))`.
    /// Prefer this over `take_action` in single-hand play; use `take_action` directly
    /// only when you need explicit hand targeting (e.g., future split scenarios).
    #[instrument(skip(self))]
    pub fn action_on_current(self, action: BasicAction) -> Result<GameResult, ActionError> {
        let idx = self.current_hand_index;
        self.take_action(PlayerAction::new(action, idx))
    }

    /// Takes an action, consuming self and transitioning to next state.
    #[instrument(skip(self))]
    pub fn take_action(self, action: PlayerAction) -> Result<GameResult, ActionError> {
        let proof = validate_action(&action, &self)?;
        let mut game = self;
        execute_action(&action, &mut game, proof)?;

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

        if self.current_hand_index >= self.num_hands {
            Ok(GameResult::DealerTurn(GameDealerTurn {
                shoe: self.shoe,
                player_hands: self.player_hands,
                num_hands: self.num_hands,
                dealer_hand: self.dealer_hand,
                bets: self.bets,
                ledger: self.ledger,
                bet_deducted: self.bet_deducted,
            }))
        } else {
            Ok(GameResult::PlayerTurn(self))
        }
    }

    /// Returns all active player hands.
    pub fn player_hands(&self) -> &[Hand] {
        &self.player_hands[..self.num_hands]
    }

    /// Returns the dealer's hand.
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

/// Game in dealer turn phase — dealer plays by fixed rules.
#[derive(Clone, Elicit)]
pub struct GameDealerTurn {
    pub(crate) shoe: Shoe,
    pub(crate) player_hands: [Hand; MAX_PLAYER_HANDS],
    pub(crate) num_hands: usize,
    pub(crate) dealer_hand: Hand,
    pub(crate) bets: [u64; MAX_PLAYER_HANDS],
    /// Financial ledger threading the BetDeducted proof to settlement.
    pub(crate) ledger: BankrollLedger,
    /// Proof token: bet was deducted; required by BankrollLedger::settle.
    pub(crate) bet_deducted: Established<BetDeducted>,
}

impl std::fmt::Debug for GameDealerTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active_hands = &self.player_hands[..self.num_hands];
        let active_bets = &self.bets[..self.num_hands];
        f.debug_struct("GameDealerTurn")
            .field("player_hands", &active_hands)
            .field("dealer_hand", &self.dealer_hand)
            .field("bets", &active_bets)
            .field("ledger_balance", &self.ledger.post_bet_balance())
            .field("bet", &self.ledger.bet())
            .field("bet_deducted", &"<proof token>")
            .finish()
    }
}

impl GameDealerTurn {
    /// Plays dealer turn and resolves game (consumes dealer turn, returns finished).
    #[instrument(skip(self))]
    pub fn play_dealer_turn(mut self) -> (GameFinished, Established<PayoutSettled>) {
        // Dealer follows fixed rules: hit on 16 or less, stand on 17+.
        //
        // Bounded for-loop (not while) so Kani can auto-determine the unwind
        // limit from MAX_HAND_CARDS without needing #[kani::unwind].
        for _ in 0..MAX_HAND_CARDS {
            if self.dealer_hand.value().best() >= 17 {
                break;
            }
            if let Some(card) = self.shoe.generate() {
                self.dealer_hand.add_card(card);
            } else {
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
    fn resolve(self) -> (GameFinished, Established<PayoutSettled>) {
        let dealer_value = self.dealer_hand.value().best();
        let dealer_bust = self.dealer_hand.is_bust();

        let mut outcomes = [Outcome::default(); MAX_PLAYER_HANDS];
        // Bounded by compile-time constant so Kani auto-determines loop bound.
        for (i, outcome) in outcomes.iter_mut().enumerate().take(self.num_hands) {
            let hand = &self.player_hands[i];
            let player_value = hand.value().best();
            *outcome = if hand.is_bust() {
                Outcome::Loss
            } else if dealer_bust || player_value > dealer_value {
                Outcome::Win
            } else if player_value < dealer_value {
                Outcome::Loss
            } else {
                Outcome::Push
            };
        }

        let primary_outcome = outcomes[0];
        let (final_bankroll, settled) = self.ledger.settle(primary_outcome, self.bet_deducted);

        (
            GameFinished {
                player_hands: self.player_hands,
                num_hands: self.num_hands,
                dealer_hand: self.dealer_hand,
                bets: self.bets,
                outcomes,
                bankroll: final_bankroll,
            },
            settled,
        )
    }
}

// ─────────────────────────────────────────────────────────────
//  Finished Phase
// ─────────────────────────────────────────────────────────────

/// Game finished — outcomes determined.
#[derive(Debug, Clone, Elicit)]
pub struct GameFinished {
    player_hands: [Hand; MAX_PLAYER_HANDS],
    num_hands: usize,
    dealer_hand: Hand,
    bets: [u64; MAX_PLAYER_HANDS],
    outcomes: [Outcome; MAX_PLAYER_HANDS],
    bankroll: u64,
}

impl GameFinished {
    /// Returns active player hands.
    pub fn player_hands(&self) -> &[Hand] {
        &self.player_hands[..self.num_hands]
    }

    /// Returns dealer hand.
    pub fn dealer_hand(&self) -> &Hand {
        &self.dealer_hand
    }

    /// Returns bets for each active hand.
    pub fn bets(&self) -> &[u64] {
        &self.bets[..self.num_hands]
    }

    /// Returns outcomes for each active hand.
    pub fn outcomes(&self) -> &[Outcome] {
        &self.outcomes[..self.num_hands]
    }

    /// Returns final bankroll.
    pub fn bankroll(&self) -> u64 {
        self.bankroll
    }
}

// ─────────────────────────────────────────────────────────────
//  Result Type
// ─────────────────────────────────────────────────────────────

/// Result of a game transition — carries the game to the next phase.
#[derive(Debug, Elicit)]
pub enum GameResult {
    /// Game in player turn phase.
    PlayerTurn(GamePlayerTurn),
    /// Game in dealer turn phase.
    DealerTurn(GameDealerTurn),
    /// Game finished — carries proof that [`BankrollLedger::settle`] ran
    /// exactly once with a valid [`BetDeducted`] token.
    Finished(GameFinished, Established<PayoutSettled>),
}
