//! Strictly Craps — Pure game logic with formal verification
//!
//! This crate provides the core craps game types, rules, typestate machine,
//! and workflow contracts. Designed for formal verification with Kani.
//!
//! ## Architecture
//!
//! - **Pure types**: DieFace, DiceRoll, Point, BetType, ActiveBet
//! - **Pure rules**: Payout math, comeout classification, point resolution
//! - **Typestate**: GameBetting → GameComeOut → GamePointPhase → GameResolved
//! - **Workflow**: Proof-carrying contract chain (BetsPlaced → PointEstablished → RoundSettled)
//! - **Progressive lessons**: Bet types gated by lesson level (1–5)

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod bet;
mod dice;
mod error;
mod ledger;
mod lesson;
mod payout;
mod personality;
mod point;
mod table;
mod typestate;
pub mod workflow;

// Core types
pub use bet::{ActiveBet, BetType, BettingAction};
pub use dice::{DiceRoll, DieFace};
pub use error::{CrapsError, CrapsErrorKind};
pub use ledger::{BetDeducted, CrapsLedger, RoundSettled};
pub use lesson::LessonProgress;
pub use payout::{BetOutcome, house_edge, payout_ratio, resolve_bet};
pub use personality::AgentPersonality;
pub use point::Point;
pub use table::{CrapsSeat, CrapsTable, SeatRoundResult};
pub use typestate::{
    ComeOutResult, GameBetting, GameComeOut, GamePointPhase, GameResolved, GameSetup,
    MAX_BETS_PER_SEAT, MAX_ROLLS_PER_ROUND, MAX_SEATS, PointRollResult,
};
pub use workflow::{
    BetsPlaced, ComeOutOutput, PointEstablished, PointRollOutput, execute_comeout_roll,
    execute_place_bets, execute_point_roll,
};
