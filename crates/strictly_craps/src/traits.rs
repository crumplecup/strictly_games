//! Trait interface for canonical craps rule enforcement.
//!
//! The [`CrapsRuleEnforcer`] trait provides the officially sanctioned route
//! for establishing proof tokens.  Implementations must honour the
//! [`ProvableFrom`](elicitation::contracts::ProvableFrom) semantics: a proof
//! token is only issued when the evidence genuinely holds.

use elicitation::contracts::Established;

use crate::ledger::RoundSettled;
use crate::workflow::BetsPlaced;
use crate::{ActiveBet, CrapsError, GameResolved};

/// Contract: types that can enforce craps rules and establish proof tokens.
///
/// Implementors gate proof issuance behind actual validation — no proof is
/// returned unless the invariant holds.
pub trait CrapsRuleEnforcer {
    /// Verifies that all seat bets are valid against the given bankrolls.
    ///
    /// A bet set is valid when:
    /// - every individual bet has a non-zero amount, and
    /// - the total wagered per seat does not exceed that seat's bankroll.
    ///
    /// Returns [`Established<BetsPlaced>`] on success.
    fn verify_bets_placed(
        &self,
        seat_bets: &[Vec<ActiveBet>],
        bankrolls: &[u64],
    ) -> Result<Established<BetsPlaced>, CrapsError>;

    /// Verifies that a round has been resolved and issues a settlement proof.
    ///
    /// A round is considered settled when it has at least one roll in its
    /// history (the final resolving roll).
    fn verify_round_settled(&self, resolved: &GameResolved) -> Established<RoundSettled>;
}
