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

/// Proposition: all five visual board rows were constructed with horizontal
/// centre alignment requested on their `ParagraphText`.
///
/// This is a *construction-time* contract proven inside
/// [`super::display::AnyGame::to_ak_nodes`] for the board modes.  Any caller
/// that receives [`elicitation::contracts::Established<BoardCentered>`] knows,
/// at compile time, that the alignment instruction was issued to the renderer.
///
/// At runtime, [`super::render_verify`] re-checks that centering actually took
/// effect by inspecting the live ratatui buffer.
#[derive(elicitation::Prop)]
pub struct BoardCentered;

impl VerifiedWorkflow for BoardCentered {}

elicitation::proof_credential! {
    /// Witness that all five board rows were constructed via `centered_plain`
    /// or `cell_row_text`, both of which set `alignment: Some(TextAlign::Center)`
    /// on the `ParagraphText::Rich` payload.
    ///
    /// Only [`super::display`] can produce this credential, keeping the proof
    /// boundary tight.
    pub(super) CenteredBoardRowsBuilt => BoardCentered;
}
