//! High-level `BlackjackWorkflow<C>` orchestrating a full hand.
//!
//! This is the single entry point for both human (`TuiCommunicator`) and AI
//! agent communicators.  The game logic is identical regardless of who is
//! driving; only the communicator differs.
//!
//! # Example
//!
//! ```rust,ignore
//! let workflow = BlackjackWorkflow::new(communicator);
//! let result = workflow.run_hand(setup, 1000).await?;
//! println!("Outcome: {:?}, final bankroll: {}", result.outcomes, result.bankroll);
//! ```

use elicitation::{ElicitCommunicator, ElicitResult, Elicitation};
use strictly_blackjack::Outcome;
use tracing::instrument;

use crate::games::blackjack::{BasicAction, GameFinished, GameSetup};

use super::tools::{
    PlayActionOutput, PlayActionResult, execute_dealer_turn, execute_place_bet, execute_play_action,
};

/// Outcome of a single hand of blackjack.
#[derive(Debug, Clone)]
pub struct HandResult {
    /// Outcome for each player hand (one entry in standard play).
    pub outcomes: Vec<Outcome>,
    /// Bankroll after this hand's payouts.
    pub bankroll: u64,
    /// The finished game state, for display / metrics.
    pub finished: GameFinished,
}

/// Workflow driver for a complete blackjack hand.
///
/// Generic over any [`ElicitCommunicator`] so the same code runs for
/// human TUI sessions and AI agent sessions.
pub struct BlackjackWorkflow<C> {
    communicator: C,
}

impl<C: ElicitCommunicator> BlackjackWorkflow<C> {
    /// Creates a new workflow with the given communicator.
    pub fn new(communicator: C) -> Self {
        Self { communicator }
    }

    /// Returns a reference to the underlying communicator.
    pub fn communicator(&self) -> &C {
        &self.communicator
    }

    /// Run a complete hand from setup to resolution.
    ///
    /// Elicits the bet amount, applies player actions until the turn is
    /// complete, runs the dealer turn, and returns the resolved outcome.
    ///
    /// # Proof chain
    ///
    /// ```text
    /// True → [elicit bet] → BetPlaced → [action loop] → PlayerTurnComplete
    ///                                                          ↓
    ///                                               [dealer turn] → PayoutSettled
    /// ```
    #[instrument(skip(self, setup))]
    pub async fn run_hand(
        &self,
        setup: GameSetup,
        initial_bankroll: u64,
    ) -> ElicitResult<HandResult> {
        let betting = setup.start_betting(initial_bankroll);

        // ── Step 1: elicit bet ────────────────────────────────────────────
        let bet = u64::elicit(&self.communicator).await?;

        let (place_output, bet_proof) = execute_place_bet(betting, bet)
            .map_err(|e| elicitation::ElicitErrorKind::Validation(e.to_string()))?;

        // ── Fast path: natural blackjack or dealer natural ────────────────
        use super::tools::PlaceBetOutput;
        let finished = match place_output {
            PlaceBetOutput::Finished(f) => f,
            PlaceBetOutput::PlayerTurn(pt) => {
                // ── Step 2: player action loop ────────────────────────────
                let mut current = pt;
                let mut current_proof = bet_proof;

                let dealer_turn: GameFinished = loop {
                    // Elicit the next action from the communicator.
                    let action = BasicAction::elicit(&self.communicator).await?;

                    match execute_play_action(current, action, current_proof)
                        .map_err(|e| elicitation::ElicitErrorKind::Validation(e.to_string()))?
                    {
                        PlayActionResult::InProgress(next, proof) => {
                            current = next;
                            current_proof = proof;
                        }
                        PlayActionResult::Complete(output, player_done_proof) => {
                            match output {
                                PlayActionOutput::Finished(f) => break f,
                                PlayActionOutput::DealerTurn(dt) => {
                                    // ── Step 3: dealer turn ───────────────
                                    let (finished, _resolved) =
                                        execute_dealer_turn(dt, player_done_proof);
                                    break finished;
                                }
                            }
                        }
                    }
                };
                dealer_turn
            }
        };

        Ok(HandResult {
            outcomes: finished.outcomes().to_vec(),
            bankroll: finished.bankroll(),
            finished,
        })
    }
}
