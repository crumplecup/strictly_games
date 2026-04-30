//! AccessKit display implementation for the TTT [`AnyGame`] wrapper.

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use strictly_tictactoe::{Board, Player, Position, Square, TttDisplayMode};
use tracing::instrument;

use crate::games::display::GameDisplay;
use crate::games::tictactoe::AnyGame;

// ── Board rendering helpers ───────────────────────────────────────────────────

fn cell_text(sq: Square) -> &'static str {
    match sq {
        Square::Occupied(Player::X) => " X ",
        Square::Occupied(Player::O) => " O ",
        Square::Empty => "   ",
    }
}

fn board_row_label(board: &Board, left: Position, mid: Position, right: Position) -> String {
    format!(
        "{}│{}│{}",
        cell_text(board.get(left)),
        cell_text(board.get(mid)),
        cell_text(board.get(right)),
    )
}

fn board_visual_lines(board: &Board) -> [String; 5] {
    use Position::*;
    [
        board_row_label(board, TopLeft, TopCenter, TopRight),
        "───┼───┼───".to_string(),
        board_row_label(board, MiddleLeft, Center, MiddleRight),
        "───┼───┼───".to_string(),
        board_row_label(board, BottomLeft, BottomCenter, BottomRight),
    ]
}

fn board_accessible_desc(board: &Board) -> String {
    use Position::*;
    let cells = [
        (TopLeft, "top-left"),
        (TopCenter, "top-center"),
        (TopRight, "top-right"),
        (MiddleLeft, "middle-left"),
        (Center, "center"),
        (MiddleRight, "middle-right"),
        (BottomLeft, "bottom-left"),
        (BottomCenter, "bottom-center"),
        (BottomRight, "bottom-right"),
    ];
    cells
        .iter()
        .map(|(pos, label)| {
            let sq = match board.get(*pos) {
                Square::Occupied(Player::X) => "X",
                Square::Occupied(Player::O) => "O",
                Square::Empty => "empty",
            };
            format!("{label}: {sq}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn game_status_text(game: &AnyGame) -> String {
    if game.is_over() {
        if let Some(winner) = game.winner() {
            let mark = if winner == Player::X { "X" } else { "O" };
            format!("Game over — {mark} wins!")
        } else {
            "Game over — Draw".to_string()
        }
    } else if let Some(player) = game.to_move() {
        let mark = if player == Player::X { "X" } else { "O" };
        format!("Player {mark} to move")
    } else {
        "Waiting…".to_string()
    }
}

/// Maps a board position to the visual row index (0, 2, or 4) in the 5-line
/// board rendering (lines 1 and 3 are `───┼───┼───` separators).
fn cursor_row_index(pos: Position) -> usize {
    use Position::*;
    match pos {
        TopLeft | TopCenter | TopRight => 0,
        MiddleLeft | Center | MiddleRight => 2,
        BottomLeft | BottomCenter | BottomRight => 4,
    }
}

// ── GameDisplay impl ──────────────────────────────────────────────────────────

impl GameDisplay for AnyGame {
    type Mode = TttDisplayMode;

    #[instrument(skip(self))]
    fn to_ak_nodes(
        &self,
        mode: &TttDisplayMode,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>) {
        let mut nodes: Vec<(NodeId, NodeJson)> = Vec::new();
        let root_id = NodeId::from(id_base);
        let mut ctr = id_base + 1;

        match mode {
            TttDisplayMode::Board | TttDisplayMode::BoardWithCursor(_) => {
                let cursor = if let TttDisplayMode::BoardWithCursor(pos) = mode {
                    Some(*pos)
                } else {
                    None
                };
                // Board article: 5 paragraph children (3 rows + 2 separators).
                // The cursor row (if any) is marked with is_selected so the
                // ratatui bridge applies a highlight style.
                let board = self.board();
                let lines = board_visual_lines(board);
                let cursor_row = cursor.map(cursor_row_index);
                let mut para_ids: Vec<NodeId> = Vec::with_capacity(5);
                for (i, line) in lines.iter().enumerate() {
                    let pid = NodeId::from(ctr);
                    ctr += 1;
                    para_ids.push(pid);
                    let mut node = NodeJson::new(Role(AkRole::Paragraph)).with_label(line.clone());
                    if cursor_row == Some(i) {
                        node = node.with_selected(true);
                    }
                    nodes.push((pid, node));
                }
                let board_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    board_id,
                    NodeJson::new(Role(AkRole::Article))
                        .with_label("Board".to_string())
                        .with_description(board_accessible_desc(board))
                        .with_children(para_ids),
                ));

                // Status paragraph.
                let status_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    status_id,
                    NodeJson::new(Role(AkRole::Paragraph)).with_label(game_status_text(self)),
                ));

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Tic-Tac-Toe".to_string())
                        .with_children(vec![board_id, status_id]),
                ));
            }
            TttDisplayMode::BoardHistory => {
                let history = self.history();
                let mut item_ids: Vec<NodeId> = Vec::with_capacity(history.len());
                for (i, &pos) in history.iter().enumerate() {
                    let pid = NodeId::from(ctr);
                    ctr += 1;
                    item_ids.push(pid);
                    let mark = if i % 2 == 0 { "X" } else { "O" };
                    nodes.push((
                        pid,
                        NodeJson::new(Role(AkRole::ListItem)).with_label(format!(
                            "Move {}: {} → {}",
                            i + 1,
                            mark,
                            pos.label()
                        )),
                    ));
                }
                let list_id = NodeId::from(ctr);
                ctr += 1;
                nodes.push((
                    list_id,
                    NodeJson::new(Role(AkRole::List))
                        .with_label(format!("Move history ({} moves)", history.len()))
                        .with_children(item_ids),
                ));

                nodes.push((
                    root_id,
                    NodeJson::new(Role(AkRole::Main))
                        .with_label("Tic-Tac-Toe — Move History".to_string())
                        .with_children(vec![list_id]),
                ));
            }
        }

        let _ = ctr; // suppress unused warning after last use
        (root_id, nodes)
    }
}
