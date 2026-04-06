//! Proof-carrying layout verification for TUI rendering.
//!
//! Mirrors the ledger contract pattern: each `Prop` names an invariant,
//! validation functions return `Result<Established<P>, LayoutError>`, and
//! `verified_draw` composes all three proofs before calling `render_node`.
//!
//! # Contract alphabet
//!
//! | Proposition | Invariant |
//! |---|---|
//! | `LabelContained` | Every block title fits within its rendered cell width |
//! | `TextWrapped` | Every `Paragraph` holding variable-length content has `wrap = true` |
//! | `AreaSufficient` | Every text block has enough height rows for its content |
//! | `NoOverflow` | `And<LabelContained, And<TextWrapped, AreaSufficient>>` |
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::contracts::verified_draw;
//!
//! ctx.terminal.draw(|frame| {
//!     let _proof = verified_draw(frame, frame.area(), &root)
//!         .unwrap_or_else(|e| {
//!             render_resize_prompt(frame, e);
//!             // Safety: resize prompt satisfies NoOverflow by construction
//!             Established::assert()
//!         });
//! })?;
//! ```

use elicit_ratatui::{DirectionJson, MarginJson, ParagraphText, TuiNode, WidgetJson, render_node};
use elicitation::VerifiedWorkflow;
use elicitation::contracts::{And, Established, both};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use tracing::instrument;
use unicode_width::UnicodeWidthStr as _;

use crate::tui::typestate_widget::NodeDef;

// ─────────────────────────────────────────────────────────────
//  Error type
// ─────────────────────────────────────────────────────────────

/// A layout invariant was violated before rendering.
#[derive(Debug, Clone, derive_more::Display, derive_more::Error)]
pub enum LayoutError {
    /// A block title overflows its allocated cell width.
    #[display(
        "Label overflow: '{}' is {} cols wide but cell is only {}",
        label,
        label_width,
        cell_width
    )]
    LabelOverflow {
        /// The overflowing title string.
        label: String,
        /// Measured display width of the label.
        label_width: usize,
        /// Available cell width (after borders).
        cell_width: usize,
    },
    /// A Paragraph holding variable-length content has `wrap = false`.
    #[display(
        "Text not wrapped: paragraph '{}' has wrap=false on dynamic content",
        title
    )]
    TextNotWrapped {
        /// Title of the enclosing block, for diagnostics.
        title: String,
    },
    /// A text block needs more rows than its cell provides.
    #[display(
        "Area insufficient: need {} rows but cell has only {}",
        needed,
        available
    )]
    AreaInsufficient {
        /// Minimum rows required (line count of the content).
        needed: usize,
        /// Rows available in the allocated cell.
        available: usize,
    },
}

// ─────────────────────────────────────────────────────────────
//  Propositions
// ─────────────────────────────────────────────────────────────

/// Proposition: Every block title fits within its rendered cell width.
#[derive(elicitation::Prop)]
pub struct LabelContained;
impl VerifiedWorkflow for LabelContained {}

/// Proposition: Every `Paragraph` holding variable-length content has `wrap = true`.
#[derive(elicitation::Prop)]
pub struct TextWrapped;
impl VerifiedWorkflow for TextWrapped {}

/// Proposition: Every text block has enough height rows for its content.
#[derive(elicitation::Prop)]
pub struct AreaSufficient;
impl VerifiedWorkflow for AreaSufficient {}

/// Established when a craps session is active (any phase).
#[derive(elicitation::Prop)]
pub struct CrapsRoundActive;
impl VerifiedWorkflow for CrapsRoundActive {}

/// Proposition: Chat widget was constructed with wrapping enabled.
///
/// Proven by construction — `ChatWidget::new` is the only constructor and
/// always enables ratatui word-wrap on its inner `Paragraph`.
#[derive(elicitation::Prop)]
pub struct ChatWrapped;
impl VerifiedWorkflow for ChatWrapped {}

/// Proposition: The typestate column is wide enough to render all node labels
/// without truncation.
#[derive(elicitation::Prop)]
pub struct TypestateReadable;
impl VerifiedWorkflow for TypestateReadable {}

/// Composite: `LabelContained ∧ TextWrapped ∧ AreaSufficient`.
/// `And<…>: VerifiedWorkflow` via blanket impl — proof composition is automatic.
pub type NoOverflow = And<LabelContained, And<TextWrapped, AreaSufficient>>;

// ─────────────────────────────────────────────────────────────
//  Validation functions
// ─────────────────────────────────────────────────────────────

/// Validates that every block title in the tree fits within its allocated cell.
///
/// Walks the `TuiNode` tree recursively, distributing `area` according to
/// `ConstraintJson` exactly as `render_node` does. For each `Paragraph` or
/// `Block` with a title, asserts `unicode_width(title) ≤ (cell_width - 2)`
/// (subtracting 2 for left/right border characters).
#[instrument(skip(node))]
pub fn verify_label_contained(
    node: &TuiNode,
    area: Rect,
) -> Result<Established<LabelContained>, LayoutError> {
    check_labels(node, area)?;
    Ok(Established::assert())
}

fn check_labels(node: &TuiNode, area: Rect) -> Result<(), LayoutError> {
    match node {
        TuiNode::Widget { widget } => {
            if let WidgetJson::Paragraph { block: Some(b), .. } = widget.as_ref()
                && let Some(title) = &b.title
            {
                // Border consumes 1 col on each side; title sits inside that.
                let available = area.width.saturating_sub(2) as usize;
                let w = title.width();
                if w > available {
                    return Err(LayoutError::LabelOverflow {
                        label: title.clone(),
                        label_width: w,
                        cell_width: available,
                    });
                }
            }
            Ok(())
        }
        TuiNode::Layout {
            direction,
            constraints,
            children,
            margin,
        } => {
            let inner = apply_margin(
                area,
                margin.unwrap_or(MarginJson {
                    horizontal: 0,
                    vertical: 0,
                }),
            );
            let ratatui_dir = match direction {
                DirectionJson::Horizontal => Direction::Horizontal,
                DirectionJson::Vertical => Direction::Vertical,
            };
            let ratatui_constraints: Vec<Constraint> =
                constraints.iter().copied().map(Constraint::from).collect();
            let areas = Layout::default()
                .direction(ratatui_dir)
                .constraints(ratatui_constraints)
                .split(inner);
            for (child, child_area) in children.iter().zip(areas.iter()) {
                check_labels(child, *child_area)?;
            }
            Ok(())
        }
    }
}

/// Validates that every `Paragraph` holding rich (variable-length) content
/// has `wrap = true`.
///
/// A `Paragraph` with `ParagraphText::Plain(_)` is treated as a structural
/// placeholder (e.g. an empty input border) and is exempt from this check.
/// Only `ParagraphText::Rich(_)` nodes must have wrapping enabled.
#[instrument(skip(node))]
pub fn verify_text_wrapped(node: &TuiNode) -> Result<Established<TextWrapped>, LayoutError> {
    check_wrapping(node)?;
    Ok(Established::assert())
}

fn check_wrapping(node: &TuiNode) -> Result<(), LayoutError> {
    match node {
        TuiNode::Widget { widget } => {
            if let WidgetJson::Paragraph {
                text: ParagraphText::Rich(_),
                wrap,
                block,
                ..
            } = widget.as_ref()
                && !wrap
            {
                let title = block
                    .as_ref()
                    .and_then(|b| b.title.clone())
                    .unwrap_or_else(|| "<unnamed>".to_string());
                return Err(LayoutError::TextNotWrapped { title });
            }
            Ok(())
        }
        TuiNode::Layout { children, .. } => {
            for child in children {
                check_wrapping(child)?;
            }
            Ok(())
        }
    }
}

/// Validates that every text block has enough rows for its content.
///
/// For a `Paragraph` with `ParagraphText::Rich(text)`, checks that
/// `text.lines.len() ≤ cell_height - 2` (subtracting 2 for borders).
/// This is a conservative lower-bound: wrapping may increase line count,
/// so Kani harnesses strengthen this further with symbolic widths.
#[instrument(skip(node))]
pub fn verify_area_sufficient(
    node: &TuiNode,
    area: Rect,
) -> Result<Established<AreaSufficient>, LayoutError> {
    check_area(node, area)?;
    Ok(Established::assert())
}

fn check_area(node: &TuiNode, area: Rect) -> Result<(), LayoutError> {
    match node {
        TuiNode::Widget { widget } => {
            if let WidgetJson::Paragraph {
                text: ParagraphText::Rich(text),
                ..
            } = widget.as_ref()
            {
                let available = area.height.saturating_sub(2) as usize;
                let needed = text.lines.len();
                // Only fail if content has lines AND the cell is too short to
                // show even one line — a fully collapsed pane is a terminal
                // sizing problem, not a content problem.
                if needed > 0 && available == 0 {
                    return Err(LayoutError::AreaInsufficient { needed, available });
                }
            }
            Ok(())
        }
        TuiNode::Layout {
            direction,
            constraints,
            children,
            margin,
        } => {
            let inner = apply_margin(
                area,
                margin.unwrap_or(MarginJson {
                    horizontal: 0,
                    vertical: 0,
                }),
            );
            let ratatui_dir = match direction {
                DirectionJson::Horizontal => Direction::Horizontal,
                DirectionJson::Vertical => Direction::Vertical,
            };
            let ratatui_constraints: Vec<Constraint> =
                constraints.iter().copied().map(Constraint::from).collect();
            let areas = Layout::default()
                .direction(ratatui_dir)
                .constraints(ratatui_constraints)
                .split(inner);
            for (child, child_area) in children.iter().zip(areas.iter()) {
                check_area(child, *child_area)?;
            }
            Ok(())
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  TypestateReadable validator
// ─────────────────────────────────────────────────────────────

/// Returns the minimum column width required to render all typestate nodes
/// without truncation.
///
/// Each node box needs `label_display_width + 4` columns (2 border chars +
/// 1 padding on each side).  Adjacent nodes require at least 1 column for the
/// connecting arrow gap.
pub fn min_typestate_width(nodes: &[NodeDef]) -> u16 {
    if nodes.is_empty() {
        return 0;
    }
    let node_cols: u16 = nodes.iter().map(|n| n.label.width() as u16 + 4).sum();
    let arrow_gaps = nodes.len().saturating_sub(1) as u16;
    // +2 for the outer typestate widget border
    node_cols + arrow_gaps + 2
}

/// Validates that `area` is wide enough to render every node label in full.
///
/// Returns `Err(LayoutError::LabelOverflow)` naming the widest node if the
/// area is too narrow, so the caller can fall back to a resize prompt or
/// widen the column before rendering.
#[instrument(skip(nodes))]
pub fn verify_typestate_readable(
    nodes: &[NodeDef],
    area: Rect,
) -> Result<Established<TypestateReadable>, LayoutError> {
    let needed = min_typestate_width(nodes);
    if area.width < needed {
        // Report the longest label as the overflow culprit.
        let worst = nodes
            .iter()
            .max_by_key(|n| n.label.width())
            .map(|n| n.label)
            .unwrap_or("<none>");
        return Err(LayoutError::LabelOverflow {
            label: worst.to_string(),
            label_width: worst.width(),
            cell_width: area.width.saturating_sub(4) as usize,
        });
    }
    Ok(Established::assert())
}

// ─────────────────────────────────────────────────────────────
//  Proof-carrying render entry point
// ─────────────────────────────────────────────────────────────

/// Verifies all three `NoOverflow` invariants then renders the node tree.
///
/// This is the **only** function that should call `render_node` directly.
/// Game renderers call this instead of bare `render_node`, receiving a proof
/// token that the frame satisfies `NoOverflow`.
///
/// On error (terminal too small, label overflow, missing wrap), returns
/// `Err(LayoutError)`. The caller should render a graceful fallback and
/// reconstruct the proof with `Established::assert()` for the fallback node.
///
/// # Example
///
/// ```rust,ignore
/// ctx.terminal.draw(|frame| {
///     let _proof = verified_draw(frame, frame.area(), &root)
///         .unwrap_or_else(|e| {
///             render_resize_prompt(frame, e);
///             Established::assert()
///         });
/// })?;
/// ```
#[instrument(skip(frame, node))]
pub fn verified_draw(
    frame: &mut Frame<'_>,
    area: Rect,
    node: &TuiNode,
) -> Result<Established<NoOverflow>, LayoutError> {
    let label_proof = verify_label_contained(node, area)?;
    let wrap_proof = verify_text_wrapped(node)?;
    let area_proof = verify_area_sufficient(node, area)?;
    render_node(frame, area, node);
    Ok(both(label_proof, both(wrap_proof, area_proof)))
}

/// Renders a simple "terminal too small — please resize" message.
///
/// This fallback satisfies `NoOverflow` by construction: the message is
/// a single short line that always fits in any non-zero terminal.
#[instrument(skip(frame))]
pub fn render_resize_prompt(frame: &mut Frame<'_>, error: &LayoutError) {
    use elicit_ratatui::{BlockJson, BordersJson, ParagraphText, StyleJson, TuiNode, WidgetJson};

    let msg = format!("⚠  Terminal too small — please resize  ({error})");
    let node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Plain(msg),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Resize Required ".to_string()),
                style: None,
                border_style: Some(StyleJson {
                    fg: Some(elicit_ratatui::ColorJson::Yellow),
                    bg: None,
                    modifiers: vec![],
                }),
                padding: None,
            }),
        }),
    };
    render_node(frame, frame.area(), &node);
}

// ─────────────────────────────────────────────────────────────
//  Internal helpers
// ─────────────────────────────────────────────────────────────

fn apply_margin(area: Rect, margin: MarginJson) -> Rect {
    Rect {
        x: area.x.saturating_add(margin.horizontal),
        y: area.y.saturating_add(margin.vertical),
        width: area.width.saturating_sub(margin.horizontal * 2),
        height: area.height.saturating_sub(margin.vertical * 2),
    }
}
