//! Proof-carrying contracts for craps using elicitation contracts.
//!
//! The manual underside layer: prop markers, evidence bundles, and
//! [`ProvableFrom`] impls that define what constitutes a valid craps round.

use elicitation::VerifiedWorkflow;
use elicitation::contracts::{Established, ProvableFrom};

use crate::BetsPlaced;

// ─────────────────────────────────────────────────────────────
//  Top-Level Invariant
// ─────────────────────────────────────────────────────────────

/// Proposition: the craps game is being played according to the rules.
///
/// Wired to [`CrapsRulesEvidence`]: formal-method harnesses call
/// `Established::prove(&CrapsConsistent::kani_proof_credential())`.
#[derive(elicitation::Prop)]
#[prop(credential = CrapsRulesEvidence)]
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

#[cfg(kani)]
impl kani::Arbitrary for CrapsRulesEvidence {
    fn any() -> Self {
        Self {
            bets_placed: Established::assert(),
        }
    }
}
