//! [`RenderVerifiable`] impls for [`BoardColumnsAligned`] and [`BoardCentered`].
//!
//! After the board `Article` node is painted into a ratatui buffer, calling
//! [`verify_in_debug`] here reads every separator character (or board row)
//! back from the buffer and `debug_assert!`s that the corresponding invariant
//! holds — the same invariant proven at construction time, now verified against
//! what was *actually rendered*.

use crate::{BoardCentered, BoardColumnsAligned};
use elicit_ratatui::{RatatuiRenderContext, RenderVerifiable};
use elicit_ui::RenderContext;
use tracing::instrument;

/// Separator characters rendered by [`crate::games::tictactoe::display`].
const SEP_CHARS: [char; 2] = ['|', '+'];

// ── RenderVerifiable: BoardColumnsAligned ─────────────────────────────────────

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
                if sym.chars().next().is_some_and(|c| SEP_CHARS.contains(&c)) {
                    sep_cols.push(col);
                }
            }
            per_row.push(sep_cols);
        }

        // All rows with separators must agree on positions.
        let non_empty: Vec<&Vec<u16>> = per_row.iter().filter(|v| !v.is_empty()).collect();

        if non_empty.is_empty() {
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
                *cols, reference,
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

// ── RenderVerifiable: BoardCentered ───────────────────────────────────────────

impl RenderVerifiable<RatatuiRenderContext<'_>> for BoardCentered {
    /// Verify that every rendered board row is centred within the area.
    ///
    /// For each row that contains at least one non-space cell, the first and
    /// last non-space column positions are found and their midpoint is compared
    /// against the area midpoint.  A tolerance of ±1 cell accommodates integer
    /// rounding in odd-width areas.
    #[instrument(skip(ctx, area), fields(area_w = ctx.area_width(area), area_h = ctx.area_height(area)))]
    fn verify_rendered(ctx: &RatatuiRenderContext<'_>, area: &ratatui::layout::Rect) {
        let height = ctx.area_height(area);
        let width = ctx.area_width(area);
        let area_mid = width / 2;

        let mut rows_checked = 0u16;

        for row in 0..height {
            // Find first and last non-space column.
            let first = (0..width).find(|&col| ctx.symbol_at(area, col, row).trim() != "");
            let last = (0..width)
                .rev()
                .find(|&col| ctx.symbol_at(area, col, row).trim() != "");

            let (Some(first), Some(last)) = (first, last) else {
                continue; // blank row — skip
            };

            rows_checked += 1;
            let row_mid = (first + last) / 2;
            let diff = area_mid.abs_diff(row_mid);

            if diff > 1 {
                tracing::error!(
                    row,
                    first_col = first,
                    last_col = last,
                    row_mid,
                    area_mid,
                    diff,
                    "BoardCentered violated: row content is not centred"
                );
            }
            debug_assert!(
                diff <= 1,
                "BoardCentered violated at row {row}: \
                 content span [{first}..{last}] midpoint {row_mid} \
                 differs from area midpoint {area_mid} by {diff} (tolerance 1)"
            );
        }

        tracing::debug!(rows_checked, area_mid, "board_centered_verify: OK");
    }
}
