//! Lobby system — multi-screen TUI with profile selection, stats, and agent selection.

mod controller;
mod screen;
mod screens;
mod settings;

pub use controller::LobbyController;
pub use screen::{Screen, ScreenTransition};
pub use settings::{FirstPlayer, GameType, LobbySettings, PlayerKind, PlayerSlot};
