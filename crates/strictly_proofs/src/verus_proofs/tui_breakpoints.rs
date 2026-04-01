//! Verus proofs for TUI NoOverflow layout contracts.
//!
//! # Design notes
//!
//! These proofs are **self-contained** — no workspace dependencies are
//! imported.  All arithmetic is expressed directly in `verus!{}` spec
//! functions over `u64` (unbounded enough to avoid saturation concerns at
//! terminal-relevant scales) and the proof functions reason purely about
//! integer inequalities.
//!
//! This matches the Verus mirror pattern used throughout this crate: Verus
//! cannot resolve workspace crates when invoked as `verus --crate-type=lib`,
//! so all domain logic is re-stated as specification functions.
//!
//! # Properties proven
//!
//! 1. **Truncation bounded** — `∀ w m: truncated_width(w, m) ≤ m`
//! 2. **Truncation identity** — `w ≤ m → truncated_width(w, m) = w`
//! 3. **Truncation satisfies LabelContained** — universally, for all inputs
//! 4. **Node-box overflow safety** — `label + 4` capped at `cols` ≤ `u16::MAX`
//!    for any terminal up to 200 cols wide
//! 5. **AreaSufficient detection** — zero height + content implies failure
//! 6. **AreaSufficient pass** — non-zero height + content ≤ available passes
//! 7. **Breakpoint arithmetic** — symbolic "must pass" range satisfies both
//!    label-truncation and row-budget invariants

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

// ─────────────────────────────────────────────────────────────
//  Specification functions
// ─────────────────────────────────────────────────────────────

/// Column width of a string after truncating to `max_cols` display columns.
///
/// Models the post-condition of `truncate_to_width` in `tui/contracts.rs`:
/// the result always fits within `max_cols` display columns.
pub open spec fn truncated_width(input_width: u64, max_cols: u64) -> u64 {
    if max_cols == 0 {
        0
    } else if input_width <= max_cols {
        input_width
    } else {
        max_cols
    }
}

/// Inner usable width of a bordered cell.
///
/// A `Block` widget consumes 1 column on each side.
pub open spec fn inner_width(cell_width: u64) -> u64 {
    if cell_width >= 2 { (cell_width - 2) as u64 } else { 0u64 }
}

/// `LabelContained` invariant: `label_width` fits inside a bordered cell.
pub open spec fn label_contained(label_width: u64, cell_width: u64) -> bool {
    label_width <= inner_width(cell_width)
}

/// `AreaSufficient` failure condition: content exists but area is zero-height.
pub open spec fn area_sufficient_fails(available: u64, needed: u64) -> bool {
    available == 0 && needed > 0
}

// ─────────────────────────────────────────────────────────────
//  Property 1 — truncation output is always ≤ max_cols
// ─────────────────────────────────────────────────────────────

pub proof fn truncation_output_bounded(input_width: u64, max_cols: u64)
    ensures truncated_width(input_width, max_cols) <= max_cols,
{
    if max_cols == 0 {
        assert(truncated_width(input_width, max_cols) == 0);
    } else if input_width <= max_cols {
        assert(truncated_width(input_width, max_cols) == input_width);
    } else {
        assert(truncated_width(input_width, max_cols) == max_cols);
    }
}

// ─────────────────────────────────────────────────────────────
//  Property 2 — truncation identity when input already fits
// ─────────────────────────────────────────────────────────────

pub proof fn truncation_identity(input_width: u64, max_cols: u64)
    requires input_width <= max_cols,
    ensures  truncated_width(input_width, max_cols) == input_width,
{
    if max_cols == 0 {
        // input_width ≤ 0 so input_width = 0; result = 0 = input_width ✓
        assert(input_width == 0);
        assert(truncated_width(input_width, max_cols) == 0);
    } else {
        assert(truncated_width(input_width, max_cols) == input_width);
    }
}

// ─────────────────────────────────────────────────────────────
//  Property 3 — truncation always satisfies LabelContained (universal)
// ─────────────────────────────────────────────────────────────

/// The canonical foundation proof for the NoOverflow contract stack.
///
/// After applying `truncate_to_width` with the inner width as the limit,
/// the resulting label always satisfies `LabelContained`.
///
/// This holds **universally** — no assumptions on `label_width` or
/// `cell_width`.  It is the arithmetic guarantee that `verified_draw`
/// relies on when constructing an `Established<LabelContained>` token.
pub proof fn truncation_always_satisfies_label_contained(label_width: u64, cell_width: u64)
    ensures label_contained(truncated_width(label_width, inner_width(cell_width)), cell_width),
{
    let inner = inner_width(cell_width);
    let after = truncated_width(label_width, inner);

    // By Property 1: after ≤ inner
    truncation_output_bounded(label_width, inner);

    // label_contained(after, cell_width) ≡ after ≤ inner_width(cell_width)
    assert(after <= inner);
    assert(label_contained(after, cell_width));
}

// ─────────────────────────────────────────────────────────────
//  Property 4 — node-box width never overflows u16::MAX
// ─────────────────────────────────────────────────────────────

/// For any terminal up to 200 columns wide, the node-box computation
/// `(label_width + 4).min(cols)` stays within `u16::MAX`.
///
/// We reason over `u64` to avoid saturation and then note that the
/// result is ≤ 200 ≤ u16::MAX (65535).
pub proof fn node_box_width_no_overflow(label_width: u64, terminal_cols: u64)
    requires
        terminal_cols <= 200,
        label_width < terminal_cols,
    ensures
        // Capped node-box width ≤ terminal_cols ≤ 200 ≤ u16::MAX
        {
            let box_w = if label_width + 4 <= terminal_cols as int {
                label_width + 4
            } else {
                terminal_cols as int
            };
            box_w <= terminal_cols as int && box_w <= 65535
        },
{
    let box_w = if label_width + 4 <= terminal_cols as int {
        label_width + 4
    } else {
        terminal_cols as int
    };
    assert(box_w <= terminal_cols as int);
    assert(terminal_cols <= 200);
    assert(box_w <= 200int);
    assert(200int <= 65535int);
    assert(box_w <= 65535int);
}

// ─────────────────────────────────────────────────────────────
//  Property 5 — AreaSufficient: zero height with content fails
// ─────────────────────────────────────────────────────────────

pub proof fn area_zero_height_triggers_failure(available: u64, needed: u64)
    requires
        available == 0,
        needed > 0,
    ensures area_sufficient_fails(available, needed),
{
    assert(area_sufficient_fails(available, needed));
}

// ─────────────────────────────────────────────────────────────
//  Property 6 — AreaSufficient: non-zero height with fitting content passes
// ─────────────────────────────────────────────────────────────

pub proof fn area_nonzero_height_does_not_fail(available: u64, needed: u64)
    requires
        available > 0,
        needed <= available,
    ensures !area_sufficient_fails(available, needed),
{
    assert(!area_sufficient_fails(available, needed));
}

// ─────────────────────────────────────────────────────────────
//  Property 7a — Minimum breakpoint (80×24) layout arithmetic
// ─────────────────────────────────────────────────────────────

/// At 80×24, four blackjack node boxes (14 cols each) + 3 gaps (59 total)
/// fit in 80 columns, and the node row (3) + prompt (10) fit in 24 rows.
pub proof fn breakpoint_minimum_layout()
    ensures
        14u64 * 4 + 3 <= 80,   // four nodes + three gaps ≤ cols
        3u64 + 10 <= 24,       // node_row_h + prompt_h ≤ rows
{
    assert(14u64 * 4 + 3 == 59u64);
    assert(59u64 <= 80u64);
    assert(3u64 + 10 == 13u64);
    assert(13u64 <= 24u64);
}

// ─────────────────────────────────────────────────────────────
//  Property 7b — Ultrawide breakpoint (200×60) layout arithmetic
// ─────────────────────────────────────────────────────────────

pub proof fn breakpoint_ultrawide_layout()
    ensures
        14u64 * 4 + 3 <= 90,   // 45% of 200 = 90 cols for graph
        3u64 + 2 + 1 + 8 + 1 + 15 + 10 <= 60,  // full row budget
{
    assert(14u64 * 4 + 3 == 59u64);
    assert(59u64 <= 90u64);
    assert(3u64 + 2 + 1 + 8 + 1 + 15 + 10 == 40u64);
    assert(40u64 <= 60u64);
}

// ─────────────────────────────────────────────────────────────
//  Property 7c — Micro breakpoint (40×12): expected failure
// ─────────────────────────────────────────────────────────────

/// At 40×12, the minimum node row (59 cols) does NOT fit in 40 cols,
/// and the minimum row budget (21) exceeds 12 rows.
pub proof fn breakpoint_micro_expected_failure()
    ensures
        14u64 * 4 + 3 > 40,   // node row too wide
        3u64 + 2 + 1 + 5 + 10 > 12,  // row budget exhausted
{
    assert(14u64 * 4 + 3 == 59u64);
    assert(59u64 > 40u64);
    assert(3u64 + 2 + 1 + 5 + 10 == 21u64);
    assert(21u64 > 12u64);
}

// ─────────────────────────────────────────────────────────────
//  Property 7d — Tiny breakpoint (60×20): graceful degrade
// ─────────────────────────────────────────────────────────────

/// At 60×20, the node row barely fits (59 ≤ 60) but the row budget (21)
/// exceeds 20 — the graceful-degrade condition is proven.
pub proof fn breakpoint_tiny_graceful_degrade()
    ensures
        14u64 * 4 + 3 <= 60,  // node row just fits in width
        3u64 + 2 + 1 + 5 + 10 > 20,  // row budget exhausted — degrade triggered
{
    assert(14u64 * 4 + 3 == 59u64);
    assert(59u64 <= 60u64);
    assert(3u64 + 2 + 1 + 5 + 10 == 21u64);
    assert(21u64 > 20u64);
}

// ─────────────────────────────────────────────────────────────
//  Property 8 — symbolic must-pass range
// ─────────────────────────────────────────────────────────────

/// For every terminal in the "must pass" range ([80..200] cols × [24..60] rows):
/// - A 10-char label fits after truncation in any 4-node slot layout.
/// - The node-row (3) + prompt (10) = 13 rows fit in any rows ≥ 24.
pub proof fn symbolic_must_pass_range(cols: u64, rows: u64)
    requires
        cols >= 80,
        cols <= 200,
        rows >= 24,
        rows <= 60,
    ensures
        // Row budget: node_row_h + prompt_h = 13 ≤ rows
        3u64 + 10 <= rows,
        // Label truncation: 10-char label fits in any slot ≥ 12 cols wide
        // Slot = cols/4 ≥ 20 at minimum (80/4=20); inner = slot-2 ≥ 18 ≥ 10 ✓
        {
            let slot = cols / 4;
            let inner = if slot >= 2 { slot - 2 } else { 0 };
            10u64 <= inner  // worst-case 10-char label fits
        },
{
    // Row budget
    assert(3u64 + 10 == 13u64);
    assert(rows >= 24);
    assert(13u64 <= rows);

    // Label truncation
    let slot = cols / 4;
    // cols ≥ 80, so slot ≥ 20
    assert(slot >= 20u64) by {
        assert(cols >= 80u64);
        // integer division: 80/4 = 20, and cols/4 is monotone
        assert(cols / 4 >= 80int / 4) by {
            assert(cols >= 80u64);
        };
    };
    let inner = if slot >= 2 { slot - 2 } else { 0 };
    // inner ≥ slot - 2 ≥ 20 - 2 = 18 ≥ 10
    assert(inner >= slot - 2);
    assert(slot - 2 >= 18u64);
    assert(inner >= 18u64);
    assert(10u64 <= inner);
}

} // verus!
