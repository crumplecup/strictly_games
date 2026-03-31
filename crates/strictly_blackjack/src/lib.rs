//! Strictly Blackjack - Pure game logic with formal verification
//!
//! This crate provides the core blackjack game types, rules, typestate machine,
//! and workflow contracts. Designed for formal verification with Kani, Verus,
//! and Creusot.
//!
//! ## Architecture
//!
//! - **Pure types**: Card, Rank, Suit, Shoe, Hand
//! - **Pure rules**: Hand value calculation, blackjack detection, bust detection
//! - **Typestate**: GameSetup → GameBetting → GamePlayerTurn → GameDealerTurn → GameFinished
//! - **Workflow**: Proof-carrying contract chain (BetPlaced → PlayerTurnComplete → PayoutSettled)
//! - **Elicitation derives**: Generates verification harness methods

#![warn(missing_docs)]

mod action;
mod card;
mod contracts;
pub mod error;
mod explore;
mod hand;
pub mod ledger;
pub mod multi_player;
pub mod rules;
mod shoe;
mod types;
mod typestate;
mod view;
pub mod workflow;

// Core types
pub use action::{BasicAction, PlayerAction};
pub use card::{Card, Rank, Suit};
pub use contracts::{LegalAction, NotBust, ValidAction, execute_action, validate_action};
pub use error::ActionError;
pub use explore::BlackjackAction;
pub use hand::{Hand, HandValue, MAX_HAND_CARDS, MAX_PLAYER_HANDS};
pub use ledger::{BankrollLedger, BetDeducted, PayoutSettled};
pub use multi_player::{MAX_SEATS, MultiRound, SeatBet, SeatPlay, SeatResult};
pub use shoe::Shoe;
pub use types::Outcome;
pub use typestate::{
    GameBetting, GameDealerTurn, GameFinished, GamePlayerTurn, GameResult, GameSetup,
};
pub use view::BlackjackPlayerView;
// Workflow re-exports for convenience
pub use workflow::{
    BetPlaced, PlaceBetOutput, PlayActionOutput, PlayActionResult, PlayerTurnComplete,
    execute_dealer_turn, execute_place_bet, execute_play_action,
};
