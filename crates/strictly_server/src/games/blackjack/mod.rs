//! Blackjack game implementation with typestate state machine and contracts.

mod action;
mod contracts;
mod typestate;
pub mod workflow;

// Local exports
pub use action::{ActionError, BasicAction, PlayerAction};
pub use typestate::{GameBetting, GameDealerTurn, GameFinished, GamePlayerTurn, GameResult, GameSetup};
pub use workflow::{
    BetPlaced, BlackjackWorkflow, HandResolved, HandResult, PlayerTurnComplete,
    PlaceBetOutput, PlayActionOutput, PlayActionResult,
    execute_dealer_turn, execute_place_bet, execute_play_action,
};
