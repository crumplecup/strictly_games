//! AccessKit display implementation for the TTT [`AnyGame`] wrapper.

use accesskit::Role as AkRole;
use elicit_accesskit::{NodeId, NodeJson, Role};
use elicit_ui::{ParagraphText, RichText, TextAlign, TextLine, TextModifier, TextSpan, TextStyle};
use elicitation::contracts::Established;
use strictly_tictactoe::{Board, Player, Position, Square, TttDisplayMode};
use tracing::{debug, instrument};
use unicode_width::UnicodeWidthStr;

use crate::games::tictactoe::contracts::CenteredBoardRowsBuilt;
use crate::{BoardCentered, BoardColumnsAligned};
use crate::games::display::GameDisplay;
use crate::games::tictactoe::AnyGame;

// ── Cell width constant ───────────────────────────────────────────────────────

/// Display-column width guaranteed for every board cell.
const CELL_WIDTH: usize = 3;

/// Separator row — `+` characters fall at the same display columns as `|` in
/// cell rows, proving [`BoardColumnsAligned`] by construction.
const SEPARATOR: &str = "---+---+---";

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
        log_separator_positions(&lines);
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
        "{}|{}|{}",
        pad_cell(cell_text(board.get(left)), CELL_WIDTH),
        pad_cell(cell_text(board.get(mid)), CELL_WIDTH),
        pad_cell(cell_text(board.get(right)), CELL_WIDTH),
    )
}

/// Log the display-column positions of every `│` / `┼` separator character
/// in the five board lines.  Call this whenever the board changes so the
/// log file records the exact column layout for post-mortem analysis.
///
/// Example log output (all positions should be identical across all rows):
/// ```text
/// board_col_positions row=0 line="   │   │   " sep_cols=[3,7]
/// board_col_positions row=1 line="───┼───┼───" sep_cols=[3,7]
/// ```
#[instrument]
fn log_separator_positions(lines: &[String; 5]) {
    for (row, line) in lines.iter().enumerate() {
        let sep_cols: Vec<usize> = sep_display_cols(line);
        debug!(
            row,
            line = %line,
            sep_cols = ?sep_cols,
            "board_col_positions",
        );
    }
}

/// Returns the display-column offsets of every `|` or `+` character in `s`.
fn sep_display_cols(s: &str) -> Vec<usize> {
    let mut cols = Vec::new();
    let mut col: usize = 0;
    for ch in s.chars() {
        if ch == '|' || ch == '+' {
            cols.push(col);
        }
        col += UnicodeWidthStr::width(ch.encode_utf8(&mut [0u8; 4]));
    }
    cols
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
/// board rendering (lines 1 and 3 are `---+---+---` separators).
fn cursor_row_index(pos: Position) -> usize {
    use Position::*;
    match pos {
        TopLeft | TopCenter | TopRight => 0,
        MiddleLeft | Center | MiddleRight => 2,
        BottomLeft | BottomCenter | BottomRight => 4,
    }
}

/// Maps a board position to its cell column (0=left, 1=center, 2=right).
fn cursor_col_index(pos: Position) -> usize {
    use Position::*;
    match pos {
        TopLeft | MiddleLeft | BottomLeft => 0,
        TopCenter | Center | BottomCenter => 1,
        TopRight | MiddleRight | BottomRight => 2,
    }
}

/// Maps visual row index (0, 2, 4) to the three board positions in that row.
///
/// Row indices 1 and 3 are separator rows — callers should never pass those.
fn row_positions(row: usize) -> (Position, Position, Position) {
    use Position::*;
    match row {
        0 => (TopLeft, TopCenter, TopRight),
        2 => (MiddleLeft, Center, MiddleRight),
        _ => (BottomLeft, BottomCenter, BottomRight),
    }
}

/// Wrap a plain string in a center-aligned `ParagraphText::Rich` so the
/// ratatui bridge renders it horizontally centered within its column.
fn centered_plain(text: &str) -> ParagraphText {
    ParagraphText::Rich(RichText {
        lines: vec![TextLine {
            spans: vec![TextSpan { content: text.to_string(), style: None }],
            style: None,
            alignment: None,
        }],
        style: None,
        alignment: Some(TextAlign::Center),
    })
}

/// Highlight style for the cursor cell.
fn cursor_style() -> TextStyle {
    TextStyle {
        modifiers: vec![TextModifier::Reversed, TextModifier::Bold],
        ..Default::default()
    }
}

/// Build a `ParagraphText` for a cell row, highlighting only the cell at
/// `cursor_col` (0, 1, or 2) when provided.
fn cell_row_text(board: &Board, left: Position, mid: Position, right: Position, cursor_col: Option<usize>) -> ParagraphText {
    let cells = [
        pad_cell(cell_text(board.get(left)), CELL_WIDTH),
        pad_cell(cell_text(board.get(mid)), CELL_WIDTH),
        pad_cell(cell_text(board.get(right)), CELL_WIDTH),
    ];
    let sep = TextSpan { content: "|".to_string(), style: None };

    let spans: Vec<TextSpan> = vec![
        TextSpan {
            content: cells[0].clone(),
            style: if cursor_col == Some(0) { Some(cursor_style()) } else { None },
        },
        sep.clone(),
        TextSpan {
            content: cells[1].clone(),
            style: if cursor_col == Some(1) { Some(cursor_style()) } else { None },
        },
        sep.clone(),
        TextSpan {
            content: cells[2].clone(),
            style: if cursor_col == Some(2) { Some(cursor_style()) } else { None },
        },
    ];

    ParagraphText::Rich(RichText {
        lines: vec![TextLine { spans, style: None, alignment: None }],
        style: None,
        alignment: Some(TextAlign::Center),
    })
}

// ── TttBoardDisplay trait ─────────────────────────────────────────────────────

/// Extension of [`GameDisplay`] for TTT board modes.
///
/// Separates the board-section construction — which carries a compile-time
/// centering proof — from the generic `to_ak_nodes` path used for non-board
/// modes such as `BoardHistory`.  Blackjack and Craps are entirely unaffected.
///
/// The canonical way to obtain [`Established<BoardCentered>`] is to call this
/// method; the proof token witnesses that every board paragraph was constructed
/// via [`centered_plain`] or [`cell_row_text`], both of which set
/// `alignment: Some(TextAlign::Center)` on their `ParagraphText::Rich` payload.
pub trait TttBoardDisplay {
    /// Build the board `Article` node and all five board-row `Paragraph` nodes,
    /// returning the board root id, all produced pairs, and a compile-time proof
    /// that center alignment was requested on every row.
    ///
    /// `cursor` is `Some(pos)` when the cursor is active, `None` for plain board.
    /// `id_base` is the first `u64` available for `NodeId` allocation.
    fn create_board_with_proof(
        &self,
        cursor: Option<Position>,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>, Established<BoardCentered>);
}

// ── TttBoardDisplay impl ──────────────────────────────────────────────────────

impl TttBoardDisplay for AnyGame {
    #[instrument(skip(self))]
    fn create_board_with_proof(
        &self,
        cursor: Option<Position>,
        id_base: u64,
    ) -> (NodeId, Vec<(NodeId, NodeJson)>, Established<BoardCentered>) {
        let mut nodes: Vec<(NodeId, NodeJson)> = Vec::new();
        let mut ctr = id_base;

        let board = self.board();
        let (aligned, _columns_proof) = AlignedBoardLines::new(board);
        let cursor_row = cursor.map(cursor_row_index);
        let cursor_col = cursor.map(cursor_col_index);

        let mut para_ids: Vec<NodeId> = Vec::with_capacity(5);
        for (i, line) in aligned.lines().iter().enumerate() {
            let pid = NodeId::from(ctr);
            ctr += 1;
            para_ids.push(pid);
            let pt = if cursor_row == Some(i) {
                let (left, mid, right) = row_positions(i);
                cell_row_text(board, left, mid, right, cursor_col)
            } else {
                centered_plain(line)
            };
            let v = serde_json::to_value(&pt).expect("ParagraphText serializable");
            let node = NodeJson::new(Role(AkRole::Paragraph))
                .with_label(line.clone())
                .with_rich_text_value(v);
            nodes.push((pid, node));
        }

        let board_id = NodeId::from(ctr);
        nodes.push((
            board_id,
            NodeJson::new(Role(AkRole::Article))
                .with_label("Board".to_string())
                .with_description(board_accessible_desc(board))
                .with_children(para_ids),
        ));

        // Every row above was built via centered_plain or cell_row_text,
        // both of which set alignment: Some(TextAlign::Center).
        let centered_proof = Established::prove(&CenteredBoardRowsBuilt);
        (board_id, nodes, centered_proof)
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
                let (board_id, board_pairs, _centered_proof) =
                    self.create_board_with_proof(cursor, ctr);
                ctr += board_pairs.len() as u64;
                nodes.extend(board_pairs);

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
