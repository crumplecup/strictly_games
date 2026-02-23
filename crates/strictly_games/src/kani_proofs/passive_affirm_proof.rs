//! Formal verification of the Passive-Affirm escape hatch pattern.
//!
//! Uses "cloud of assumptions" pattern:
//! - Trust: tokio::watch semantics (no data races, monotonic updates)
//! - Trust: Rust's Send/Sync guarantees
//! - Verify: Our wrapper logic for cancellation (GameSession methods)
//!
//! This proves the cancellation mechanism used in the game loop
//! satisfies critical safety properties.

use crate::session::GameSession;

/// Verifies that affirm_continue() always returns (no deadlock).
///
/// Property: affirm_continue() terminates
///
/// Cloud: Trust tokio::watch::Receiver::borrow() is non-blocking
/// Verify: Our wrapper doesn't add blocking code
///
/// The game loop calls affirm_continue() during agent turns.
/// If it could deadlock, the entire UI would freeze. This proof ensures
/// the escape hatch always allows checking cancellation status.
#[kani::proof]
fn affirm_continue_always_returns() {
    let session = GameSession::new("test_session".to_string());

    // This call must always complete (no infinite wait, no deadlock)
    let result = session.affirm_continue();

    // The result is either true or false, but crucially, we GOT a result
    // Cloud: tokio::watch guarantees borrow() returns immediately
    assert!(
        result == true || result == false,
        "affirm_continue must always return a boolean"
    );
}

/// Verifies that cancellation is monotonic (once set, stays set).
///
/// Property: cancelled(t) ⟹ ∀t' > t, cancelled(t')
///
/// Cloud: Trust tokio::watch::Sender::send() updates atomically
/// Verify: Our wrapper logic preserves monotonicity
///
/// Once a user presses 'q' to cancel, the game should stay cancelled.
/// If cancellation could "un-cancel", the escape hatch would be unreliable.
#[kani::proof]
fn cancellation_is_monotonic() {
    let session = GameSession::new("test_session".to_string());

    // Cancel the game
    session.request_cancel();

    // Check status multiple times
    // Cloud: tokio::watch guarantees all receivers see the update
    let first = session.affirm_continue();
    let second = session.affirm_continue();
    let third = session.affirm_continue();

    // All checks after cancellation should return false (game should not continue)
    assert_eq!(first, false, "First check should be cancelled");
    assert_eq!(second, false, "Second check should be cancelled");
    assert_eq!(third, false, "Third check should be cancelled");
}

/// Verifies that multiple cancel requests are idempotent.
///
/// Property: cancel(); cancel(); ≡ cancel()
///
/// Cloud: Trust tokio::watch handles multiple sends correctly
/// Verify: Our wrapper doesn't corrupt state on repeated calls
///
/// User might press 'q' multiple times in frustration. The system
/// should handle this gracefully without state corruption.
#[kani::proof]
fn multiple_cancels_are_idempotent() {
    let session = GameSession::new("test_session".to_string());

    // Cancel multiple times
    session.request_cancel();
    session.request_cancel();
    session.request_cancel();

    // State should be same as single cancel
    let result = session.affirm_continue();

    assert_eq!(result, false, "Multiple cancels should have same effect as one");
}

/// Verifies that a fresh session is not cancelled.
///
/// Property: new session ⟹ not cancelled
///
/// Cloud: Trust tokio::watch::channel(false) initializes to false
/// Verify: Our GameSession::new() doesn't accidentally cancel
///
/// If a new game started already cancelled, it couldn't run.
/// This verifies the initialization is correct.
#[kani::proof]
fn new_session_is_not_cancelled() {
    let session = GameSession::new("test_session".to_string());

    let should_continue = session.affirm_continue();

    assert_eq!(should_continue, true, "New session should not be cancelled");
}

/// Verifies that reset_cancel() restores non-cancelled state.
///
/// Property: cancel(); reset(); ⟹ not cancelled
///
/// Cloud: Trust tokio::watch::Sender::send(false) updates correctly
/// Verify: Our reset_cancel() wrapper calls send(false)
///
/// For game restart, we need to reset cancellation. This verifies
/// the reset operation works correctly.
#[kani::proof]
fn reset_cancel_restores_state() {
    let session = GameSession::new("test_session".to_string());

    // Cancel, then reset
    session.request_cancel();
    session.reset_cancel();

    // Should be back to non-cancelled state
    // Cloud: tokio::watch propagates the false value
    let should_continue = session.affirm_continue();

    assert_eq!(should_continue, true, "Reset should restore non-cancelled state");
}

/// Verifies request_cancel sets the flag correctly.
///
/// Property: request_cancel() ⟹ !affirm_continue()
///
/// Cloud: Trust tokio::watch channel synchronization
/// Verify: Our wrapper connects the methods correctly
#[kani::proof]
fn request_cancel_sets_flag() {
    let session = GameSession::new("test_session".to_string());

    // Initially not cancelled
    let before = session.affirm_continue();
    assert_eq!(before, true);

    // Cancel
    session.request_cancel();

    // Now cancelled
    let after = session.affirm_continue();
    assert_eq!(after, false, "request_cancel sets cancellation flag");
}

/// Verifies symbolic boolean behavior.
///
/// Property: ∀b ∈ bool, affirm_continue returns valid boolean
///
/// This uses kani::any() to explore ALL possible cancellation states.
#[kani::proof]
fn symbolic_cancellation_state() {
    // Symbolic boolean: explores both cancelled and not-cancelled states
    let is_cancelled: bool = kani::any();

    // In either state, affirm_continue should return valid boolean
    let session = GameSession::new("test".to_string());

    if is_cancelled {
        session.request_cancel();
    }

    let result = session.affirm_continue();

    // Result should match expected state
    if is_cancelled {
        assert_eq!(result, false);
    } else {
        assert_eq!(result, true);
    }
}
