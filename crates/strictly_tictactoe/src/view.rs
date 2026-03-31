//! Tic-tac-toe game state view for agent exploration.
//!
//! [`TicTacToeView`] snapshots the visible board state. Its [`ElicitSpec`]
//! impl registers categories that map 1:1 to the explore variants in
//! [`TicTacToeAction`](crate::TicTacToeAction).

use elicitation::{
    ElicitSpec, SpecCategoryBuilder, SpecEntryBuilder, TypeSpec, TypeSpecBuilder,
    TypeSpecInventoryKey,
};
use tracing::instrument;

use crate::rules::{check_winner, is_full};
use crate::{Board, Player, Position, Square};

/// Snapshot of visible tic-tac-toe state at a player's turn.
#[derive(Debug, Clone)]
pub struct TicTacToeView {
    board_display: String,
    legal_moves: Vec<Position>,
    threats: Vec<String>,
}

impl TicTacToeView {
    /// Builds a view snapshot from live board state.
    #[instrument(skip(board))]
    pub fn from_board(board: &Board, current_player: Player) -> Self {
        let board_display = format_board(board);
        let legal_moves = Position::valid_moves(board);
        let threats = find_threats(board, current_player);

        Self {
            board_display,
            legal_moves,
            threats,
        }
    }

    /// Formats the response for a given explore category.
    #[instrument(skip(self))]
    pub fn describe_category(&self, category: &str) -> Option<String> {
        match category {
            "board" => Some(self.board_display.clone()),
            "legal_moves" => {
                if self.legal_moves.is_empty() {
                    Some("No legal moves — board is full".to_string())
                } else {
                    let labels: Vec<&str> = self.legal_moves.iter().map(|p| p.label()).collect();
                    Some(format!(
                        "{} open positions: {}",
                        self.legal_moves.len(),
                        labels.join(", ")
                    ))
                }
            }
            "threats" => {
                if self.threats.is_empty() {
                    Some("No immediate win or block opportunities".to_string())
                } else {
                    Some(self.threats.join("\n"))
                }
            }
            _ => None,
        }
    }
}

/// Formats the board as a readable 3×3 grid.
#[instrument(skip(board))]
fn format_board(board: &Board) -> String {
    let squares = board.squares();
    let mut lines = Vec::with_capacity(5);

    for row in 0..3 {
        let cells: Vec<String> = (0..3)
            .map(|col| {
                let idx = row * 3 + col;
                match squares[idx] {
                    Square::Empty => {
                        let pos = Position::from_index(idx).expect("valid index");
                        pos.label().to_string()
                    }
                    Square::Occupied(Player::X) => "X".to_string(),
                    Square::Occupied(Player::O) => "O".to_string(),
                }
            })
            .collect();
        lines.push(cells.join(" | "));
        if row < 2 {
            lines.push("-----------".to_string());
        }
    }

    lines.join("\n")
}

/// Finds immediate win and block opportunities.
#[instrument(skip(board))]
fn find_threats(board: &Board, current_player: Player) -> Vec<String> {
    let mut threats = Vec::new();
    let opponent = current_player.opponent();

    let winning_lines: [[usize; 3]; 8] = [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8],
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8],
        [0, 4, 8],
        [2, 4, 6],
    ];

    let squares = board.squares();

    for line in &winning_lines {
        let mut player_count = 0;
        let mut opponent_count = 0;
        let mut empty_pos = None;

        for &idx in line {
            match squares[idx] {
                Square::Occupied(p) if p == current_player => player_count += 1,
                Square::Occupied(_) => opponent_count += 1,
                Square::Empty => empty_pos = Some(idx),
            }
        }

        if let Some(idx) = empty_pos {
            let pos = Position::from_index(idx).expect("valid index");
            if player_count == 2 && opponent_count == 0 {
                threats.push(format!("WIN: Play {} to complete three in a row", pos));
            } else if opponent_count == 2 && player_count == 0 {
                threats.push(format!(
                    "BLOCK: {:?} can win at {} — must block",
                    opponent, pos
                ));
            }
        }
    }

    if check_winner(board).is_some() {
        threats.push("Game already has a winner".to_string());
    } else if is_full(board) {
        threats.push("Board is full — game is a draw".to_string());
    }

    threats
}

impl ElicitSpec for TicTacToeView {
    fn type_spec() -> TypeSpec {
        let board = SpecCategoryBuilder::default()
            .name("board".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("grid".to_string())
                    .description("3×3 board showing X, O, and open positions".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let legal_moves = SpecCategoryBuilder::default()
            .name("legal_moves".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("positions".to_string())
                    .description("All unoccupied board positions available to play".to_string())
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        let threats = SpecCategoryBuilder::default()
            .name("threats".to_string())
            .entries(vec![
                SpecEntryBuilder::default()
                    .label("win_opportunities".to_string())
                    .description("Positions where you can complete three in a row".to_string())
                    .build()
                    .expect("valid SpecEntry"),
                SpecEntryBuilder::default()
                    .label("block_needed".to_string())
                    .description(
                        "Positions where opponent threatens to win — must block".to_string(),
                    )
                    .build()
                    .expect("valid SpecEntry"),
            ])
            .build()
            .expect("valid SpecCategory");

        TypeSpecBuilder::default()
            .type_name("TicTacToeView".to_string())
            .summary(
                "Visible board state during a tic-tac-toe turn — grid, moves, threats".to_string(),
            )
            .categories(vec![board, legal_moves, threats])
            .build()
            .expect("valid TypeSpec")
    }
}

elicitation::inventory::submit!(TypeSpecInventoryKey::new(
    "TicTacToeView",
    <TicTacToeView as ElicitSpec>::type_spec,
    std::any::TypeId::of::<TicTacToeView>
));
