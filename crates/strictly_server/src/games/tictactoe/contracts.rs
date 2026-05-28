//! Display-layer proof propositions for tic-tac-toe.
//!
//! These propositions govern the visual rendering contract, not the game rules.
//! They live here (in the server/display layer) rather than in `strictly_tictactoe`
//! because they are meaningless without a rendering frontend.

use elicitation::VerifiedWorkflow;

/// Proposition: all five visual lines of the tic-tac-toe board have their
/// vertical separator characters (`|` / `+`) at identical display-column
/// positions across every row.
///
/// This is a *rendering* contract proven by construction inside
/// [`super::display::AlignedBoardLines::new`].  Any function that receives
/// [`elicitation::contracts::Established<BoardColumnsAligned>`] knows, at
/// compile time, that the board strings are free of column-shift artefacts
/// regardless of which marks have been placed.
///
/// At runtime, [`super::render_verify`] re-checks this invariant against the
/// live ratatui buffer via [`elicit_ui::RenderVerifiable`].
#[derive(elicitation::Prop)]
pub struct BoardColumnsAligned;

impl VerifiedWorkflow for BoardColumnsAligned {}
