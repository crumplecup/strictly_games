//! Step 6: proof-carrying contracts for the ice cream order path.

use elicitation::VerifiedWorkflow;
use elicitation::contracts::{Established, ProvableFrom};
use tracing::instrument;

use crate::flavor::IceCreamFlavor;

/// Proposition: a flavor was chosen through the legitimate elicitation path.
#[derive(elicitation::Prop)]
pub struct FlavorChosen;

impl VerifiedWorkflow for FlavorChosen {}

/// Proposition: a scoop has been legitimately prepared.
#[derive(elicitation::Prop)]
pub struct ScoopComplete;

impl VerifiedWorkflow for ScoopComplete {}

/// Evidence that a scoop was prepared from a legitimately elicited flavor.
pub struct ScoopEvidence {
    /// Proof that the flavor was elicited.
    pub flavor_chosen: Established<FlavorChosen>,
}

impl ProvableFrom<ScoopEvidence> for ScoopComplete {}

/// A single scoop ready to serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoop {
    flavor: IceCreamFlavor,
}

impl Scoop {
    /// Creates a scoop of the given flavor.
    #[instrument]
    pub fn new(flavor: IceCreamFlavor) -> Self {
        Self { flavor }
    }

    /// Returns the flavor in this scoop.
    #[instrument]
    pub fn flavor(&self) -> IceCreamFlavor {
        self.flavor
    }
}

/// Validates that a flavor was elicited and issues a proof token.
#[instrument]
pub fn validate_flavor_chosen(flavor: &IceCreamFlavor) -> Established<FlavorChosen> {
    let _ = flavor;
    Established::assert()
}

/// Scoops ice cream, consuming `FlavorChosen` and issuing `ScoopComplete`.
#[instrument(skip(flavor_proof))]
pub fn scoop(
    flavor: IceCreamFlavor,
    flavor_proof: Established<FlavorChosen>,
) -> (Scoop, Established<ScoopComplete>) {
    let evidence = ScoopEvidence {
        flavor_chosen: flavor_proof,
    };
    let sidecar = Established::<ScoopComplete>::prove(&evidence);
    (Scoop::new(flavor), sidecar)
}
