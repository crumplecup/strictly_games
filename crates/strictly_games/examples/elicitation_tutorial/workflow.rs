//! Elicitation pipeline: elicit → validate → scoop with proof sidecar.

use elicitation::{ElicitCommunicator, ElicitResult, Elicitation};
use tracing::instrument;

use crate::contracts::{Scoop, ScoopComplete, scoop, validate_flavor_chosen};
use crate::flavor::IceCreamFlavor;

/// Result of a complete ice cream order.
pub struct OrderResult {
    /// The prepared scoop.
    pub scoop: Scoop,
    /// Proof that the scoop came from a legitimately elicited flavor.
    pub proof: elicitation::contracts::Established<ScoopComplete>,
}

/// Generic workflow over any real [`ElicitCommunicator`] (`TuiCommunicator` or `ElicitClient`).
pub struct IceCreamWorkflow<C> {
    communicator: C,
}

impl<C: ElicitCommunicator> IceCreamWorkflow<C> {
    /// Creates a workflow bound to the given communicator.
    #[instrument(skip(communicator))]
    pub fn new(communicator: C) -> Self {
        Self { communicator }
    }

    /// Elicit a flavor, validate it, scoop it, and return the proof sidecar.
    #[instrument(skip(self))]
    pub async fn take_order(&self) -> ElicitResult<OrderResult> {
        let flavor = IceCreamFlavor::elicit(&self.communicator).await?;
        let flavor_proof = validate_flavor_chosen(&flavor);
        let (scoop, proof) = scoop(flavor, flavor_proof);
        Ok(OrderResult { scoop, proof })
    }
}
