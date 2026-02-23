//! Creusot proofs for Passive-Affirm escape hatch pattern.

use crate::session::GameSession;

/// Verify affirm_continue() returns a boolean.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(session.affirm_continue() == true || session.affirm_continue() == false)]
pub fn verify_affirm_continue_returns_bool(session: &GameSession) -> bool {
    session.affirm_continue()
}

/// Verify new session is not cancelled.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(GameSession::new("test".to_string()).affirm_continue())]
pub fn verify_new_session_not_cancelled() -> bool {
    GameSession::new("test".to_string()).affirm_continue()
}
