//! Multi-player blackjack table engine.
//!
//! A single [`Shoe`] is shared across all seats and the dealer.
//! Each seat carries its own [`BankrollLedger`] and `Established<BetDeducted>`
//! proof token, preserving the single-seat financial invariants at scale: a
//! bet is deducted exactly once per seat and settlement occurs exactly once.
//!
//! # Round flow
//!
//! ```text
//! 1. Caller collects one SeatBet per player (via elicitation).
//! 2. MultiRound::deal(bets, seed) → debit each bankroll, deal in casino rotation.
//! 3. Check dealer_natural(): if true, settle immediately (skip player turns).
//! 4. For each seat (!is_done()): elicit Hit/Stand; call seat.hit()/seat.stand().
//! 5. multi_round.play_dealer()
//! 6. multi_round.settle() → Vec<SeatResult> with final bankrolls.
//! ```
//!
//! # Proof chain per seat
//!
//! ```text
//! bankroll, bet
//!     ↓
//! BankrollLedger::debit()  ──► Established<BetDeducted>   (inside SeatPlay)
//!                                          ↓
//!                        SeatPlay::settle(dealer_hand)
//!                                          ↓
//!                             (final_bankroll, Established<PayoutSettled>)
//! ```

use elicitation::Generator;
use elicitation::contracts::Established;
use tracing::instrument;

use crate::MAX_HAND_CARDS;
use crate::{ActionError, BankrollLedger, BetDeducted, Hand, Outcome, Shoe};

// ─────────────────────────────────────────────────────────────
//  Constants
// ─────────────────────────────────────────────────────────────

/// Maximum number of seats at a multi-player table (3 AI agents + 1 human).
pub const MAX_SEATS: usize = 4;

// ─────────────────────────────────────────────────────────────
//  Input: seat bet requests
// ─────────────────────────────────────────────────────────────

/// Seat bet request — one per player before the round begins.
///
/// Constructed by the caller after eliciting a bet from each player.
#[derive(Debug, Clone)]
pub struct SeatBet {
    /// Display name shown in the TUI (e.g. "You", "GPT-4o").
    pub name: String,
    /// Current bankroll for this player.
    pub bankroll: u64,
    /// Amount the player is wagering this hand.
    pub bet: u64,
}

// ─────────────────────────────────────────────────────────────
//  Active seat during play
// ─────────────────────────────────────────────────────────────

/// One seat's active hand state during a round.
///
/// Carries the [`BankrollLedger`] and `Established<BetDeducted>` proof token
/// so that settlement is provably tied to a validated prior debit.
#[derive(Debug, Clone)]
pub struct SeatPlay {
    /// Display name for the TUI.
    pub name: String,
    /// Current cards held by this seat.
    pub hand: Hand,
    /// Bet placed for this hand.
    pub bet: u64,
    ledger: BankrollLedger,
    bet_deducted: Established<BetDeducted>,
    /// `true` when the seat was dealt a natural blackjack (21 on two cards).
    pub natural: bool,
    /// `true` when the seat's hand value exceeds 21.
    pub bust: bool,
    /// `true` when the player has chosen to stand.
    pub stood: bool,
}

impl SeatPlay {
    /// Deals one card from `shoe` into this seat's hand.
    ///
    /// Sets `bust` to `true` if the hand exceeds 21.
    /// Has no effect (returns `Ok`) if the seat is already [`is_done`][Self::is_done].
    ///
    /// Takes `&Shoe` (not `&mut`) because [`Shoe`] uses interior mutability
    /// via [`Generator::generate(&self)`](elicitation::Generator::generate).
    ///
    /// # Errors
    ///
    /// Returns [`ActionError::DeckExhausted`] if the shoe has no more cards.
    #[instrument(skip(self, shoe), fields(seat = %self.name))]
    pub fn hit(&mut self, shoe: &Shoe) -> Result<(), ActionError> {
        if self.is_done() {
            return Ok(());
        }
        let card = shoe.generate().ok_or(ActionError::DeckExhausted)?;
        self.hand.add_card(card);
        if self.hand.is_bust() {
            self.bust = true;
            tracing::debug!(seat = %self.name, value = self.hand.value().best(), "Bust");
        }
        Ok(())
    }

    /// Records that the player has chosen to stand.
    #[instrument(skip(self), fields(seat = %self.name))]
    pub fn stand(&mut self) {
        tracing::debug!(seat = %self.name, "Stand");
        self.stood = true;
    }

    /// `true` when no further player actions are possible or needed.
    ///
    /// A seat is done if it has a natural, is bust, or the player has stood.
    pub fn is_done(&self) -> bool {
        self.natural || self.bust || self.stood
    }

    /// Settles this seat's hand against `dealer_hand`.
    ///
    /// Consumes `Established<BetDeducted>` via [`BankrollLedger::settle`],
    /// proving that settlement happens exactly once after a validated debit.
    ///
    /// # Natural resolution
    ///
    /// - Natural vs dealer natural → `Push` (original bet returned).
    /// - Natural vs dealer non-natural → `Blackjack` (3:2 payout).
    #[instrument(skip(self, dealer_hand), fields(seat = %self.name))]
    pub fn settle(self, dealer_hand: &Hand) -> SeatResult {
        let dealer_bust = dealer_hand.is_bust();
        let dealer_natural = dealer_hand.is_blackjack();

        let outcome = if self.natural {
            if dealer_natural {
                Outcome::Push
            } else {
                Outcome::Blackjack
            }
        } else if self.bust {
            Outcome::Loss
        } else {
            let player_value = self.hand.value().best();
            let dealer_value = dealer_hand.value().best();
            if dealer_bust || player_value > dealer_value {
                Outcome::Win
            } else if player_value < dealer_value {
                Outcome::Loss
            } else {
                Outcome::Push
            }
        };

        let (final_bankroll, _settled) = self.ledger.settle(outcome, self.bet_deducted);

        tracing::info!(
            seat = %self.name,
            ?outcome,
            bet = self.bet,
            final_bankroll,
            "Seat settled"
        );

        SeatResult {
            name: self.name,
            hand: self.hand,
            bet: self.bet,
            outcome,
            final_bankroll,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Settlement result
// ─────────────────────────────────────────────────────────────

/// Final result for a single seat after the round is complete.
#[derive(Debug, Clone)]
pub struct SeatResult {
    /// Display name for the TUI.
    pub name: String,
    /// Final hand (for display).
    pub hand: Hand,
    /// Bet placed for this hand.
    pub bet: u64,
    /// Outcome against the dealer.
    pub outcome: Outcome,
    /// Bankroll after payout.
    pub final_bankroll: u64,
}

// ─────────────────────────────────────────────────────────────
//  Multi-player round
// ─────────────────────────────────────────────────────────────

/// Active multi-player round: shared shoe, all seats, and the dealer's hand.
///
/// Created via [`MultiRound::deal`]; consumed via [`MultiRound::settle`].
pub struct MultiRound {
    /// Shared shoe serving all seats and the dealer.
    pub shoe: Shoe,
    /// Active seats in deal order.
    pub seats: Vec<SeatPlay>,
    /// Dealer's hand (face-up card visible; second card hidden until dealer plays).
    pub dealer_hand: Hand,
}

impl MultiRound {
    /// Deals a new round from `bets`.
    ///
    /// Cards are distributed in casino rotation:
    /// - Round 1: seat[0].card1, seat[1].card1, …, dealer.card1
    /// - Round 2: seat[0].card2, seat[1].card2, …, dealer.card2
    ///
    /// Each bet is validated and debited atomically before any cards are dealt.
    /// Seats with a natural (blackjack on deal) are flagged as done immediately.
    ///
    /// # Errors
    ///
    /// Returns [`ActionError`] if any bet is invalid/insufficient or the shoe
    /// runs out mid-deal (structurally impossible with a fresh 52-card shoe and
    /// ≤ [`MAX_SEATS`] players, but guarded against for safety).
    #[cfg(feature = "shuffle")]
    #[instrument(skip(bets), fields(num_seats = bets.len()))]
    pub fn deal(bets: Vec<SeatBet>, seed: u64) -> Result<Self, ActionError> {
        let shoe = Shoe::new(seed, 1);
        Self::deal_with_shoe(bets, shoe)
    }

    /// Deals a new round using a pre-built shoe (for testing / formal verification).
    #[instrument(skip(bets, shoe), fields(num_seats = bets.len()))]
    pub fn deal_with_shoe(bets: Vec<SeatBet>, shoe: Shoe) -> Result<Self, ActionError> {
        let num_seats = bets.len();

        // Debit all bankrolls before any cards are dealt so we don't deal to a
        // player whose bet turns out to be invalid.
        let mut seats: Vec<SeatPlay> = Vec::with_capacity(num_seats);
        for seat_bet in bets {
            let (ledger, bet_deducted) = BankrollLedger::debit(seat_bet.bankroll, seat_bet.bet)?;
            seats.push(SeatPlay {
                name: seat_bet.name,
                hand: Hand::empty(),
                bet: seat_bet.bet,
                ledger,
                bet_deducted,
                natural: false,
                bust: false,
                stood: false,
            });
        }

        let mut dealer_hand = Hand::empty();

        // Round 1: one card to every seat, then one to the dealer.
        for seat in &mut seats {
            let card = shoe.generate().ok_or(ActionError::DeckExhausted)?;
            seat.hand.add_card(card);
        }
        let card = shoe.generate().ok_or(ActionError::DeckExhausted)?;
        dealer_hand.add_card(card);

        // Round 2: second card to every seat, then second to the dealer.
        for seat in &mut seats {
            let card = shoe.generate().ok_or(ActionError::DeckExhausted)?;
            seat.hand.add_card(card);
        }
        let card = shoe.generate().ok_or(ActionError::DeckExhausted)?;
        dealer_hand.add_card(card);

        // Flag naturals.
        for seat in &mut seats {
            if seat.hand.is_blackjack() {
                seat.natural = true;
                tracing::info!(seat = %seat.name, "Natural blackjack on deal");
            }
        }

        tracing::info!(
            num_seats,
            dealer_up_card = ?dealer_hand,
            "Round dealt"
        );

        Ok(Self {
            shoe,
            seats,
            dealer_hand,
        })
    }

    /// `true` if the dealer was dealt a natural (blackjack) on the initial deal.
    ///
    /// When `true`, all non-natural seats lose immediately; the caller should
    /// skip player turns and call [`settle`][Self::settle] directly.
    pub fn dealer_natural(&self) -> bool {
        self.dealer_hand.is_blackjack()
    }

    /// Plays the dealer's turn using fixed casino rules: hit on ≤ 16, stand on ≥ 17.
    ///
    /// Must be called after all player turns are complete.
    #[instrument(skip(self), fields(dealer_value = self.dealer_hand.value().best()))]
    pub fn play_dealer(&mut self) {
        // Bounded loop so Kani can determine unroll depth from MAX_HAND_CARDS.
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
        tracing::debug!(
            dealer_value = self.dealer_hand.value().best(),
            bust = self.dealer_hand.is_bust(),
            "Dealer turn complete"
        );
    }

    /// Settles all seats against the dealer's final hand.
    ///
    /// Consumes `self` and returns one [`SeatResult`] per seat in deal order.
    #[instrument(skip(self))]
    pub fn settle(self) -> Vec<SeatResult> {
        let dealer_hand = self.dealer_hand;
        self.seats
            .into_iter()
            .map(|seat| seat.settle(&dealer_hand))
            .collect()
    }
}
