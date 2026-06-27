//! Step 8: verified stoplight state machine — Yellow cannot jump to Green.

use elicitation::VerifiedWorkflow;
use elicitation::contracts::Established;
use elicitation::{Elicit, KaniCompose, KaniVariantState, VerifiedStateMachine, formal_method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// State of a traffic stoplight.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
    KaniVariantState,
    KaniCompose,
)]
pub enum StoplightState {
    /// Green — traffic flows freely.
    Green,
    /// Yellow — prepare to stop.
    Yellow,
    /// Red — traffic stopped.
    Red,
}

impl Default for StoplightState {
    fn default() -> Self {
        Self::Red
    }
}

/// Proposition: the stoplight is in a consistent phase of its cycle.
#[derive(elicitation::Prop)]
pub struct StoplightConsistent;

impl VerifiedWorkflow for StoplightConsistent {}

/// Verified state machine marker — transitions listed for harness generation.
#[derive(VerifiedStateMachine)]
#[vsm(transitions = [advance_to_yellow, advance_to_red, advance_to_green])]
pub struct StoplightMachine;

/// Green → Yellow when the green phase lasted at least 30 seconds.
#[formal_method(contracts = [StoplightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_yellow(
    state: StoplightState,
    proof: Established<StoplightConsistent>,
    elapsed_seconds: u32,
) -> (StoplightState, Established<StoplightConsistent>) {
    let StoplightState::Green = state else {
        return (state, proof);
    };
    if elapsed_seconds < 30 {
        return (StoplightState::Green, proof);
    }
    (StoplightState::Yellow, proof)
}

/// Yellow → Red when the yellow phase lasted at least 5 seconds.
#[formal_method(contracts = [StoplightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_red(
    state: StoplightState,
    proof: Established<StoplightConsistent>,
    elapsed_seconds: u32,
) -> (StoplightState, Established<StoplightConsistent>) {
    let StoplightState::Yellow = state else {
        return (state, proof);
    };
    if elapsed_seconds < 5 {
        return (StoplightState::Yellow, proof);
    }
    (StoplightState::Red, proof)
}

/// Red → Green — no time guard.
#[formal_method(contracts = [StoplightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_green(
    state: StoplightState,
    proof: Established<StoplightConsistent>,
) -> (StoplightState, Established<StoplightConsistent>) {
    let StoplightState::Red = state else {
        return (state, proof);
    };
    (StoplightState::Green, proof)
}
