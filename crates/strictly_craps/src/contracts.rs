//! Proof-carrying contracts for craps using elicitation contracts.
//!
//! The manual underside layer: prop markers, evidence bundles, and
//! [`ProvableFrom`] impls that define what constitutes a valid craps round.

use elicitation::VerifiedWorkflow;
use elicitation::contracts::{Established, ProvableFrom};

use crate::{BetsPlaced, CrapsError, CrapsErrorKind};

// ─────────────────────────────────────────────────────────────
//  Bankrolls Proposition
// ─────────────────────────────────────────────────────────────

/// Proposition: the bankrolls list is non-empty.
///
/// Obtaining this token is the only way to call `craps_start_betting`.
/// The issuing function [`validate_non_empty_bankrolls`] is the "unsafe block"
/// that upholds the invariant: it checks `bankrolls.len() > 0` before asserting.
#[derive(elicitation::Prop)]
pub struct NonEmptyBankrolls;
impl VerifiedWorkflow for NonEmptyBankrolls {}

/// Validates that the bankrolls slice is non-empty and issues a proof token.
///
/// This is the single point of trust for the `NonEmptyBankrolls` invariant,
/// satisfying the craps invariant `shooter_idx@ < bankrolls@.len()` (shooter
/// starts at 0, so `bankrolls.len() >= 1` is required).
pub fn validate_non_empty_bankrolls(
    bankrolls: &[u64],
) -> Result<Established<NonEmptyBankrolls>, CrapsError> {
    if !bankrolls.is_empty() {
        Ok(Established::assert())
    } else {
        Err(CrapsError::new(CrapsErrorKind::EmptyBankrolls))
    }
}

// ─────────────────────────────────────────────────────────────
//  Top-Level Invariant
// ─────────────────────────────────────────────────────────────

/// Proposition: the craps game is being played according to the rules.
///
/// Wired to [`CrapsRulesEvidence`]: formal-method harnesses call
/// `Established::prove(&CrapsConsistent::kani_proof_credential())`.
#[derive(elicitation::Prop)]
#[prop(
    credential = CrapsRulesEvidence,
    kani_invariant_fn = "craps_consistent",
    creusot_invariant_fn = "craps_consistent",
    creusot_inv_body = "match state { CrapsState::Setup { inner, .. } => inner.num_seats@ > 0, CrapsState::Betting { inner, .. } => inner.shooter_idx@ < inner.bankrolls@.len(), CrapsState::ComeOut { inner, .. } => inner.shooter_idx@ < inner.bankrolls@.len(), CrapsState::PointPhase { inner, .. } => inner.shooter_idx@ < inner.bankrolls@.len(), CrapsState::Resolved { inner, .. } => inner.shooter_idx@ < inner.bankrolls@.len(), }",
    verus_invariant_fn = "craps_consistent",
    verus_inv_body = "match *state { CrapsState::Setup { num_seats, .. } => num_seats@ > 0, CrapsState::Betting { shooter_idx, bankrolls, .. } => shooter_idx@ < bankrolls@.len(), CrapsState::ComeOut { shooter_idx, bankrolls, .. } => shooter_idx@ < bankrolls@.len(), CrapsState::PointPhase { shooter_idx, bankrolls, .. } => shooter_idx@ < bankrolls@.len(), CrapsState::Resolved { shooter_idx, bankrolls, .. } => shooter_idx@ < bankrolls@.len(), }",
    verus_state_body = "Setup { num_seats: usize }, Betting { shooter_idx: usize, bankrolls: Vec<u64> }, ComeOut { shooter_idx: usize, bankrolls: Vec<u64> }, PointPhase { shooter_idx: usize, bankrolls: Vec<u64> }, Resolved { shooter_idx: usize, bankrolls: Vec<u64> },"
)]
pub struct CrapsConsistent;

impl VerifiedWorkflow for CrapsConsistent {}

/// Evidence bundle for establishing [`CrapsConsistent`].
///
/// Assembling this forces proof that:
/// - bets have been placed and validated against bankrolls before any roll.
///
/// This is the core craps rule: you cannot roll dice without having bets on
/// the table.
pub struct CrapsRulesEvidence {
    /// Proof that bets have been placed and validated.
    pub bets_placed: Established<BetsPlaced>,
}

impl ProvableFrom<CrapsRulesEvidence> for CrapsConsistent {}

/// Evidence bundle for establishing [`CrapsConsistent`] in the `Betting` state.
///
/// Assembling this forces proof that the bankrolls list is non-empty,
/// satisfying `shooter_idx@ < bankrolls@.len()` (shooter starts at index 0).
pub struct CrapsBettingEvidence {
    /// Proof that the bankrolls list is non-empty.
    pub non_empty: Established<NonEmptyBankrolls>,
}

impl ProvableFrom<CrapsBettingEvidence> for CrapsConsistent {}

#[cfg(kani)]
impl kani::Arbitrary for CrapsRulesEvidence {
    fn any() -> Self {
        Self {
            bets_placed: Established::assert(),
        }
    }
}
