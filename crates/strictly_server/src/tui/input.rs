//! Cursor movement for keyboard navigation.

use crate::games::tictactoe::Position;
use crossterm::event::KeyCode;

/// Moves cursor based on arrow keys.
pub fn move_cursor(cursor: Position, key: KeyCode) -> Position {
    use Position::*;

    match (cursor, key) {
        // Right movement
        (TopLeft, KeyCode::Right) => TopCenter,
        (TopCenter, KeyCode::Right) => TopRight,
        (MiddleLeft, KeyCode::Right) => Center,
        (Center, KeyCode::Right) => MiddleRight,
        (BottomLeft, KeyCode::Right) => BottomCenter,
        (BottomCenter, KeyCode::Right) => BottomRight,

        // Left movement
        (TopCenter, KeyCode::Left) => TopLeft,
        (TopRight, KeyCode::Left) => TopCenter,
        (Center, KeyCode::Left) => MiddleLeft,
        (MiddleRight, KeyCode::Left) => Center,
        (BottomCenter, KeyCode::Left) => BottomLeft,
        (BottomRight, KeyCode::Left) => BottomCenter,

        // Down movement
        (TopLeft, KeyCode::Down) => MiddleLeft,
        (TopCenter, KeyCode::Down) => Center,
        (TopRight, KeyCode::Down) => MiddleRight,
        (MiddleLeft, KeyCode::Down) => BottomLeft,
        (Center, KeyCode::Down) => BottomCenter,
        (MiddleRight, KeyCode::Down) => BottomRight,

        // Up movement
        (MiddleLeft, KeyCode::Up) => TopLeft,
        (Center, KeyCode::Up) => TopCenter,
        (MiddleRight, KeyCode::Up) => TopRight,
        (BottomLeft, KeyCode::Up) => MiddleLeft,
        (BottomCenter, KeyCode::Up) => Center,
        (BottomRight, KeyCode::Up) => MiddleRight,

        // No change for other keys or edge cases
        _ => cursor,
    }
}
