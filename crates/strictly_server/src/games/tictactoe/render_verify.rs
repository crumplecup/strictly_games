//! [`RenderVerifiable`] impl for [`BoardColumnsAligned`].
//!
//! After the board `Article` node is painted into a ratatui buffer, calling
//! [`verify_in_debug`] here reads every separator character back from the
//! buffer and `debug_assert!`s that they fall at the same columns in all five
//! rows — the same invariant that [`AlignedBoardLines`] proves at construction
//! time, now verified against what was *actually rendered*.
//!
//! This is the "runtime proof mode" for TTT board alignment: a post-hoc check
//! that survives layout regressions, wrap-trim bugs, or any future rendering
//! change that could quietly break the on-screen invariant.

use elicit_ratatui::{RatatuiRenderContext, RenderVerifiable};
use elicit_ui::RenderContext;
use crate::{BoardColumnsAligned};
use tracing::instrument;

/// Separator characters rendered by [`crate::games::tictactoe::display`].
const SEP_CHARS: [char; 2] = ['|', '+'];

// ── RenderVerifiable impl ─────────────────────────────────────────────────────

impl RenderVerifiable<RatatuiRenderContext<'_>> for BoardColumnsAligned {
    #[instrument(skip(ctx, area), fields(area_w = ctx.area_width(area), area_h = ctx.area_height(area)))]
    fn verify_rendered(ctx: &RatatuiRenderContext<'_>, area: &ratatui::layout::Rect) {
        let height = ctx.area_height(area);
        let width = ctx.area_width(area);

        // Collect separator column positions per row.
        let mut per_row: Vec<Vec<u16>> = Vec::with_capacity(height as usize);
        for row in 0..height {
            let mut sep_cols: Vec<u16> = Vec::new();
            for col in 0..width {
                let sym = ctx.symbol_at(area, col, row);
                if sym.chars().next().map_or(false, |c| SEP_CHARS.contains(&c)) {
                    sep_cols.push(col);
                }
            }
            per_row.push(sep_cols);
        }

        // All rows with separators must agree on positions.
        let non_empty: Vec<&Vec<u16>> = per_row.iter().filter(|v| !v.is_empty()).collect();

        if non_empty.is_empty() {
            // No separators found at all — the board may not have been rendered
            // into this area yet, or the area is wrong.  Log and return rather
            // than false-asserting on an empty frame.
            tracing::warn!(
                area = ?area,
                "board_columns_aligned_verify: no separator chars found in area; \
                 skipping assert (board may not be rendered yet)"
            );
            return;
        }

        let reference = non_empty[0];
        for (row_idx, cols) in non_empty.iter().enumerate() {
            if *cols != reference {
                tracing::error!(
                    row = row_idx,
                    expected = ?reference,
                    actual = ?cols,
                    "BoardColumnsAligned violated: separator positions differ across rows"
                );
            }
            debug_assert_eq!(
                *cols,
                reference,
                "BoardColumnsAligned violated at row {row_idx}: \
                 expected separators at {reference:?}, got {cols:?}"
            );
        }

        tracing::debug!(
            sep_cols = ?reference,
            rows_checked = non_empty.len(),
            "board_columns_aligned_verify: OK"
        );
    }
}
