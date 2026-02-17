//! Lobby system — multi-screen TUI with profile selection, stats, and agent selection.

mod controller;
mod screen;
mod screens;

pub use controller::LobbyController;
pub use screen::{Screen, ScreenTransition};
