//! Lobby system — multi-screen TUI with profile selection, stats, and agent selection.

mod controller;
pub(crate) mod lobby_ir;
pub(crate) mod screen;
pub(crate) mod screens;
pub(crate) mod settings;

pub use controller::LobbyController;
pub use screen::{Screen, ScreenTransition};
pub use settings::{FirstPlayer, GameType, LobbySettings, PlayerKind, PlayerSlot};
