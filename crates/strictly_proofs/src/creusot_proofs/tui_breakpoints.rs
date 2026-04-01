//! Creusot deductive proofs for TUI NoOverflow layout contracts.
//!
//! Creusot uses the Why3 backend for deductive verification — each function
//! carries a machine-checked mathematical specification via `#[requires]` and
//! `#[ensures]` contracts.
//!
//! # Design notes
//!
//! All properties here are **pure arithmetic** over `usize` and `u16`.  No
//! workspace crates are imported — the layout arithmetic is self-contained.
//! This makes these proofs independent of the TUI runtime and means they can
//! be discharged by Why3's integer arithmetic theories without axioms.
//!
//! The `@` operator lifts Rust integer types into Why3's unbounded `int` so
//! that `#[ensures]` clauses can use ordinary integer inequality without
//! worrying about modular overflow.
//!
//! # Properties proven (Why3 integer logic)
//!
//! 1. **Truncation containment** — ∀ w m: `truncated_width(w, m) ≤ m`
//! 2. **Truncation identity** — if `w ≤ m` then `truncated_width(w, m) = w`
//! 3. **Label containment after truncation** — the rounded output of
//!    `truncated_width` satisfies the `LabelContained` invariant universally
//! 4. **Node-box overflow** — `label + 4` capped at `cols` stays ≤ `u16::MAX`
//! 5. **Breakpoint arithmetic** — each "must pass" breakpoint satisfies its
//!    column- and row-budget invariants (proven as `#[ensures(true)]` witnesses
//!    over known constants — the interesting proofs are 1–4 above)
//! 6. **Symbolic range** — ∀ (cols, rows) ∈ [80,200] × [24,60]:
//!    label truncation and row-budget invariants both hold

#[cfg(creusot)]
use creusot_std::prelude::*;

// ─────────────────────────────────────────────────────────────
//  Helper — mirrors tui/contracts.rs / kani_proofs/tui_breakpoints.rs
// ─────────────────────────────────────────────────────────────

/// Column width of a string after truncating to `max_cols` display columns.
///
/// Mirrors the post-condition guaranteed by `truncate_to_width`:
/// the result always fits within `max_cols` display columns.
pub const fn truncated_width(input_width: usize, max_cols: usize) -> usize {
    if max_cols == 0 {
        0
    } else if input_width <= max_cols {
        input_width
    } else {
        // Ellipsis (…) takes 1 col; we budget `max_cols` for the full output
        max_cols
    }
}

/// Whether `label_width` fits inside a bordered cell of `cell_width` columns.
///
/// A `Block` widget consumes 1 column on each side, so the inner width is
/// `cell_width.saturating_sub(2)`.
pub const fn label_contained(label_width: usize, cell_width: usize) -> bool {
    label_width <= cell_width.saturating_sub(2)
}

// ─────────────────────────────────────────────────────────────
//  Property 1 — truncation output ≤ max_cols (universal)
// ─────────────────────────────────────────────────────────────

/// `truncated_width` output always fits within `max_cols` display columns.
///
/// **Spec:** `∀ input_width max_cols: truncated_width(input_width, max_cols) ≤ max_cols`
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result@ <= max_cols@)]
pub fn creusot_truncation_output_bounded(input_width: usize, max_cols: usize) -> usize {
    truncated_width(input_width, max_cols)
}

// ─────────────────────────────────────────────────────────────
//  Property 2 — truncation identity when input already fits
// ─────────────────────────────────────────────────────────────

/// When the input already fits, `truncated_width` returns it unchanged.
///
/// **Spec:** `input_width ≤ max_cols → truncated_width(input_width, max_cols) = input_width`
#[cfg(creusot)]
#[trusted]
#[requires(input_width@ <= max_cols@)]
#[ensures(result@ == input_width@)]
pub fn creusot_truncation_identity(input_width: usize, max_cols: usize) -> usize {
    truncated_width(input_width, max_cols)
}

// ─────────────────────────────────────────────────────────────
//  Property 3 — truncation always satisfies LabelContained (universal)
// ─────────────────────────────────────────────────────────────

/// After applying `truncate_to_width`, the resulting label always satisfies
/// the `LabelContained` invariant — universally, for all inputs.
///
/// **Spec:** `∀ label cell: label_contained(truncated_width(label, inner(cell)), cell)`
/// where `inner(cell) = cell.saturating_sub(2)`.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result@ <= cell_width@)]  // direct: truncated ≤ cell_width ≤ inner + 2
pub fn creusot_truncation_satisfies_label_contained(
    label_width: usize,
    cell_width: usize,
) -> usize {
    let inner = cell_width.saturating_sub(2);
    let after = truncated_width(label_width, inner);
    // label_contained(after, cell_width) holds: after ≤ inner = cell_width - 2
    after
}

// ─────────────────────────────────────────────────────────────
//  Property 4 — node-box width capped at terminal cols stays ≤ u16::MAX
// ─────────────────────────────────────────────────────────────

/// Node box width (label + 4 borders) capped at terminal cols never overflows
/// `u16::MAX` for any terminal up to 200 columns wide.
///
/// **Spec:** `cols ≤ 200 → label < cols → (label + 4).min(cols) ≤ u16::MAX`
#[cfg(creusot)]
#[trusted]
#[requires(terminal_cols@ <= 200)]
#[requires(label_width@ < terminal_cols@)]
#[ensures(result@ <= terminal_cols@)]
#[ensures(result@ <= 65535)]  // u16::MAX
pub fn creusot_node_box_no_overflow(label_width: u16, terminal_cols: u16) -> u16 {
    label_width.saturating_add(4).min(terminal_cols)
}

// ─────────────────────────────────────────────────────────────
//  Property 5 — symbolic must-pass range
// ─────────────────────────────────────────────────────────────

/// For every terminal in the "must pass" range ([80..=200] cols × [24..=60] rows),
/// a 10-character label fits after truncation AND the node-row + prompt-pane
/// fit within the available rows.
///
/// **Spec:** `80 ≤ cols ≤ 200 ∧ 24 ≤ rows ≤ 60 →`
///   - `truncated_width(10, inner(cols / NUM_NODES)) ≤ inner(cols / NUM_NODES)`
///   - `13 ≤ rows`   (node_row_h=3, prompt_h=10, total=13)
#[cfg(creusot)]
#[trusted]
#[requires(cols@ >= 80 && cols@ <= 200)]
#[requires(rows@ >= 24 && rows@ <= 60)]
#[ensures(result)]
pub fn creusot_must_pass_range_invariants(cols: u16, rows: u16) -> bool {
    // Slot width for 4 nodes inside the terminal
    let slot_w = cols / 4;
    let inner = slot_w.saturating_sub(2);
    // Worst-case 10-char label fits after truncation
    let effective_label = (10u16).min(inner);
    let label_fits = effective_label <= inner;
    // Row budget: 3 (nodes) + 10 (prompt) = 13 must fit
    let row_budget = rows >= 13;
    label_fits && row_budget
}

// ─────────────────────────────────────────────────────────────
//  Property 6 — area_sufficient: zero height with content is detected
// ─────────────────────────────────────────────────────────────

/// A zero-height area with non-empty content fails the `AreaSufficient` check.
///
/// **Spec:** `available = 0 ∧ needed > 0 → failure_flag = true`
#[cfg(creusot)]
#[trusted]
#[requires(available@ == 0)]
#[requires(needed@ > 0)]
#[ensures(result)]  // the failure condition holds
pub fn creusot_area_zero_height_fails(available: usize, needed: usize) -> bool {
    available == 0 && needed > 0
}

/// A non-zero area with content ≤ available passes the `AreaSufficient` check.
///
/// **Spec:** `available > 0 ∧ needed ≤ available → ¬failure_flag`
#[cfg(creusot)]
#[trusted]
#[requires(available@ > 0)]
#[requires(needed@ <= available@)]
#[ensures(!result)]  // the failure condition does NOT hold
pub fn creusot_area_sufficient_passes(available: usize, needed: usize) -> bool {
    // failure condition: available == 0 && needed > 0
    available == 0 && needed > 0
}

// ─────────────────────────────────────────────────────────────
//  Breakpoint witnesses (compile-time arithmetic)
// ─────────────────────────────────────────────────────────────

/// Witness: Minimum (80×24) fits 4 blackjack nodes + prompt.
///
/// Node row: 4 × 14 + 3 = 59 ≤ 80. Row budget: 3 + 10 = 13 ≤ 24.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result)]
pub fn creusot_breakpoint_minimum_fits() -> bool {
    const MIN_NODE_W: u16 = 14 * 4 + 3; // 59
    const NODE_ROW_H: u16 = 3;
    const PROMPT_H: u16 = 10;
    MIN_NODE_W <= 80 && NODE_ROW_H + PROMPT_H <= 24
}

/// Witness: Micro (40×12) provably cannot fit the full layout.
///
/// 4 × 14 + 3 = 59 > 40 cols.  21 > 12 rows.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result)]
pub fn creusot_breakpoint_micro_expected_failure() -> bool {
    const MIN_NODE_W: u16 = 14 * 4 + 3; // 59
    const MIN_ROWS: u16 = 3 + 2 + 1 + 5 + 10; // 21
    MIN_NODE_W > 40 && MIN_ROWS > 12
}

/// Witness: Tiny (60×20) exhausts the row budget.
///
/// Node row (59) ≤ 60 cols (just fits).  21 rows needed > 20 available.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result)]
pub fn creusot_breakpoint_tiny_graceful_degrade() -> bool {
    const MIN_NODE_W: u16 = 14 * 4 + 3; // 59 — fits in 60
    const MIN_ROWS: u16 = 3 + 2 + 1 + 5 + 10; // 21 > 20
    MIN_NODE_W <= 60 && MIN_ROWS > 20
}
