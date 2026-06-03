//! Regression test: `BoardWithCursor` must not trigger `AreaInsufficient`.
//!
//! Run with `-- --nocapture` to see the tracing output showing exactly which
//! node and area caused a failure.

use elicit_ratatui::{ParagraphText, RatatuiBackend, TuiNode, WidgetJson};
use elicit_ui::{UiTreeRenderer as _, Viewport};
use ratatui::layout::Rect;
use strictly_server::tui::contracts::verify_area_sufficient;
use strictly_server::tui::game_ir::{EventLog, GraphParams, ttt_to_verified_tree};
use strictly_server::AnyGame;
use strictly_tictactoe::{Board, Position, TttDisplayMode};
use tracing::info;

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init();
}

fn fresh_game() -> AnyGame {
    AnyGame::Setup {
        board: Board::default(),
    }
}

fn run_layout_check(label: &str, w: u16, h: u16, cursor: Position) {
    let game = fresh_game();
    let viewport = Viewport::new(w as u32, h as u32);
    let area = Rect::new(0, 0, w, h);

    info!("{label}: building VerifiedTree for {w}x{h}, cursor={cursor:?}");

    let tree = ttt_to_verified_tree(
        &game,
        &TttDisplayMode::BoardWithCursor(cursor),
        &EventLog { events: &[], dialogue: &[] },
        &GraphParams { nodes: &[], edges: &[], active: None },
        viewport,
    );

    let backend = RatatuiBackend::new();
    let (tui_node, stats, _proof) = backend
        .render(&tree)
        .expect("RatatuiBackend::render should succeed");

    info!(
        "{label}: render stats — nodes_visited={} widgets={} containers={}",
        stats.nodes_visited, stats.widgets_rendered, stats.containers_rendered
    );

    let result = verify_area_sufficient(&tui_node, area);

    info!("{label}: verify_area_sufficient result = {result:?}");

    assert!(
        result.is_ok(),
        "{label}: verify_area_sufficient failed on {w}x{h} terminal: {:?}",
        result.err()
    );
}

#[test]
fn board_with_cursor_normal_terminal() {
    init_tracing();
    run_layout_check("normal(220x50)", 220, 50, Position::TopLeft);
}

/// Compact but valid terminal (120×30).
#[test]
fn board_with_cursor_compact_terminal() {
    init_tracing();
    run_layout_check("compact(120x30)", 120, 30, Position::Center);
}

// ── Alignment diagnostic helpers ─────────────────────────────────────────────

/// Walk the TuiNode tree depth-first, calling `f` on each `WidgetJson::Paragraph`.
fn walk_paragraphs<F>(node: &TuiNode, depth: usize, f: &mut F)
where
    F: FnMut(usize, &ParagraphText, &Option<String>),
{
    match node {
        TuiNode::Widget { widget } => {
            if let WidgetJson::Paragraph { text, alignment, .. } = widget.as_ref() {
                f(depth, text, alignment);
            }
        }
        TuiNode::Layout { children, .. } => {
            for child in children {
                walk_paragraphs(child, depth + 1, f);
            }
        }
        TuiNode::StatusBar { .. } => {}
    }
}

/// Render the TTT board and log every paragraph's text alignment values,
/// returning a list of `(text_json_alignment, outer_paragraph_alignment)` pairs.
fn collect_paragraph_alignments(w: u16, h: u16) -> Vec<(Option<String>, Option<String>)> {
    let game = fresh_game();
    let viewport = Viewport::new(w as u32, h as u32);

    let tree = ttt_to_verified_tree(
        &game,
        &TttDisplayMode::BoardWithCursor(Position::Center),
        &EventLog { events: &[], dialogue: &[] },
        &GraphParams { nodes: &[], edges: &[], active: None },
        viewport,
    );

    let backend = RatatuiBackend::new();
    let (tui_node, _stats, _proof) = backend
        .render(&tree)
        .expect("RatatuiBackend::render should succeed");

    let mut results: Vec<(Option<String>, Option<String>)> = Vec::new();

    walk_paragraphs(&tui_node, 0, &mut |depth, text, outer_align| {
        let text_align = match text {
            ParagraphText::Rich(t) => t.alignment.as_ref().map(|a| format!("{a:?}")),
            ParagraphText::Plain(_) => None,
        };
        let plain_preview = text.to_plain_string();
        info!(
            depth,
            text_preview = %plain_preview.chars().take(20).collect::<String>(),
            text_align = ?text_align,
            outer_align = ?outer_align,
            "paragraph_alignment",
        );
        results.push((text_align, outer_align.clone()));
    });

    results
}

/// Verify that a genuinely tiny terminal still triggers `AreaInsufficient`,
/// confirming the check hasn't been disabled.
#[test]
fn board_with_cursor_genuinely_too_small() {
    init_tracing();
    let game = fresh_game();
    let viewport = Viewport::new(40, 10);
    let area = Rect::new(0, 0, 40, 10);

    info!("too_small(40x10): building VerifiedTree for 40x10, cursor=Center");

    let tree = ttt_to_verified_tree(
        &game,
        &TttDisplayMode::BoardWithCursor(Position::Center),
        &EventLog { events: &[], dialogue: &[] },
        &GraphParams { nodes: &[], edges: &[], active: None },
        viewport,
    );

    let backend = RatatuiBackend::new();
    let (tui_node, _stats, _proof) = backend
        .render(&tree)
        .expect("RatatuiBackend::render should succeed");

    let result = verify_area_sufficient(&tui_node, area);
    info!("too_small(40x10): result = {result:?}");
    assert!(
        result.is_err(),
        "expected AreaInsufficient for a 40×10 terminal, got Ok"
    );
}

/// Diagnostic: inspect what alignment values the render pipeline places on
/// board-row `WidgetJson::Paragraph` nodes.
///
/// Run with `-- --nocapture` to see the full tracing log.
///
/// This test catches a regression where `render_backend.rs` hardcodes
/// `alignment: None` on the outer `WidgetJson::Paragraph`, preventing
/// `Paragraph::alignment(Center)` from being called in terminal_tools.
#[test]
fn board_paragraph_alignment_is_center() {
    init_tracing();
    let alignments = collect_paragraph_alignments(220, 50);
    assert!(
        !alignments.is_empty(),
        "Expected board paragraphs in rendered tree; got none"
    );
    // Board-row paragraphs carry text_align=Some("Center"); the outer
    // WidgetJson::Paragraph.alignment must match so Paragraph::alignment()
    // is called by terminal_tools.  Non-board paragraphs (status, etc.)
    // legitimately have no alignment set.
    let mismatched: Vec<_> = alignments
        .iter()
        .filter(|(text_align, outer_align)| {
            text_align.is_some() && text_align != outer_align
        })
        .collect();
    assert!(
        mismatched.is_empty(),
        "Paragraphs with text_align set are missing matching outer_align.\n\
         (text_align, outer_align) mismatches:\n{mismatched:#?}\n\n\
         Root cause: render_backend.rs bridge_paragraph must propagate\n\
         TextJson.alignment to WidgetJson::Paragraph.alignment."
    );
}
