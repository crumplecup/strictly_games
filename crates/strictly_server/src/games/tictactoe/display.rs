//! AccessKit display implementation for the TTT [`AnyGame`] wrapper.

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use elicitation::contracts::Established;
use strictly_tictactoe::{
    Board, BoardColumnsAligned, Player, Position, Square, TttDisplayMode,
};
use tracing::instrument;
use unicode_width::UnicodeWidthStr;

use crate::games::display::GameDisplay;
use crate::games::tictactoe::AnyGame;

// ── Cell width constant ───────────────────────────────────────────────────────

/// Display-column width guaranteed for every board cell.
const CELL_WIDTH: usize = 3;

/// Separator row — `┼` characters fall at the same display columns as `│` in
/// cell rows, proving [`BoardColumnsAligned`] by construction.
const SEPARATOR: &str = "───┼───┼───";

// ── AlignedBoardLines ─────────────────────────────────────────────────────────

/// Five visual lines of the board whose column positions are guaranteed to
/// align.
///
/// This type is the proof carrier for [`BoardColumnsAligned`].  It is
/// intentionally opaque — the only way to obtain an instance is through
/// [`AlignedBoardLines::new`], which pads every cell to exactly
/// [`CELL_WIDTH`] display columns using [`unicode_width`].  The returned
/// [`Established<BoardColumnsAligned>`] witnesses that the `│`/`┼`
/// separators occupy identical terminal columns in all five rows.
pub struct AlignedBoardLines([String; 5]);

impl AlignedBoardLines {
    /// Construct the five display lines from `board`, proving
    /// [`BoardColumnsAligned`] by construction.
    ///
    /// Every cell is padded to exactly [`CELL_WIDTH`] display columns so that
    /// vertical separators never shift when marks are placed on the board.
    pub fn new(board: &Board) -> (Self, Established<BoardColumnsAligned>) {
        use Position::*;
        let lines = [
            cell_row(board, TopLeft, TopCenter, TopRight),
            SEPARATOR.to_string(),
            cell_row(board, MiddleLeft, Center, MiddleRight),
            SEPARATOR.to_string(),
            cell_row(board, BottomLeft, BottomCenter, BottomRight),
        ];
        (AlignedBoardLines(lines), Established::assert())
    }

    /// The underlying five strings, in top-to-bottom order.
    pub fn lines(&self) -> &[String; 5] {
        &self.0
    }
}

// ── Cell helpers ──────────────────────────────────────────────────────────────

fn cell_text(sq: Square) -> &'static str {
    match sq {
        Square::Occupied(Player::X) => " X ",
        Square::Occupied(Player::O) => " O ",
        Square::Empty => "   ",
    }
}

/// Pad `s` to exactly `width` display columns (unicode-aware).
fn pad_cell(s: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    if w >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - w))
    }
}

fn cell_row(board: &Board, left: Position, mid: Position, right: Position) -> String {
    format!(
        "{}│{}│{}",
        pad_cell(cell_text(board.get(left)), CELL_WIDTH),
        pad_cell(cell_text(board.get(mid)), CELL_WIDTH),
        pad_cell(cell_text(board.get(right)), CELL_WIDTH),
    )
}

// ── Accessible description ────────────────────────────────────────────────────

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
                // Build the five board lines with compile-time alignment proof.
                // AlignedBoardLines::new is the sole constructor; obtaining
                // Established<BoardColumnsAligned> here guarantees that the │/┼
                // separators occupy the same terminal columns in all five rows.
                let board = self.board();
                let (aligned, _proof) = AlignedBoardLines::new(board);
                let cursor_row = cursor.map(cursor_row_index);
                let mut para_ids: Vec<NodeId> = Vec::with_capacity(5);
                for (i, line) in aligned.lines().iter().enumerate() {
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

        let _ = ctr;
        (root_id, nodes)
    }
}
