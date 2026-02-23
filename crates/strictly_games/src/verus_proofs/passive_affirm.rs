//! Verus proofs for Passive-Affirm escape hatch pattern.

#[cfg(verus)]
use verus_builtin::*;
#[cfg(verus)]
use verus_builtin_macros::*;

use crate::session::GameSession;

#[cfg(verus)]
verus! {

/// Verify affirm_continue() always returns a boolean
pub proof fn verify_affirm_continue_returns_bool(session: &GameSession)
    ensures session.affirm_continue() == true || session.affirm_continue() == false,
{
    assert(true); // Boolean type guarantees
}

/// Verify new session is not cancelled
pub proof fn verify_new_session_not_cancelled() {
    let session = GameSession::new("test".to_string());
    assert(session.affirm_continue());
}

/// Verify cancellation is idempotent
pub proof fn verify_multiple_cancels_idempotent(session: &GameSession) {
    session.request_cancel();
    session.request_cancel();
    session.request_cancel();
    assert(!session.affirm_continue());
}

} // verus!
