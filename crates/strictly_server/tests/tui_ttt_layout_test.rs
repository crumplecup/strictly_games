//! Regression test: `BoardWithCursor` must not trigger `AreaInsufficient`.
//!
//! Run with `-- --nocapture` to see the tracing output showing exactly which
//! node and area caused a failure.

use elicit_ratatui::RatatuiBackend;
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
