//! Runnable companion to `ELICITATION_TUTORIAL.md`.
//!
//! Exercises the tutorial types on real elicitation machinery — no mocks.
//!
//! # Interactive TUI (human at the keyboard)
//!
//! ```bash
//! cargo run -p strictly_games --example elicitation_tutorial
//! ```
//!
//! Uses [`TuiCommunicator`] from `elicit_ratatui`: numbered select menu, real
//! `elicit_select` MCP tool calls, same path as the Strictly Games TUI.
//!
//! # MCP stdio (agent / Claude CLI)
//!
//! ```bash
//! cargo run -p strictly_games --example elicitation_tutorial -- --mcp
//! ```
//!
//! Uses [`ElicitClient`] over MCP stdio transport — same pattern as the
//! `enums` example in the elicitation crate. Invoke from an MCP client:
//!
//! ```text
//! claude "Run the strictly_games elicitation_tutorial example with --mcp"
//! ```

mod contracts;
mod flavor;
mod stoplight;
mod workflow;

use std::io;
use std::sync::Arc;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use elicit_ratatui::TuiCommunicator;
use elicitation::contracts::Established;
use elicitation::{ElicitClient, ElicitErrorKind, ElicitResult};
use rmcp::ServiceExt;
use tracing::instrument;

use contracts::ScoopComplete;
use stoplight::{StoplightConsistent, StoplightState, advance_to_green, advance_to_red, advance_to_yellow};
use workflow::IceCreamWorkflow;

/// How to connect the elicitation round-trip.
#[derive(Debug, Parser)]
struct Args {
    /// Use MCP stdio transport (for an external agent) instead of the TUI.
    #[arg(long)]
    mcp: bool,
}

#[tokio::main]
async fn main() -> ElicitResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter("elicitation_tutorial=info,elicitation=warn")
        .init();

    let args = Args::parse();

    if args.mcp {
        run_mcp_session().await?;
    } else {
        run_tui_session().await?;
    }

    demo_stoplight_vsm();
    Ok(())
}

/// Human session: terminal raw mode + [`TuiCommunicator`].
#[instrument]
async fn run_tui_session() -> ElicitResult<()> {
    enable_raw_mode().map_err(|e| ElicitErrorKind::Validation(e.to_string()))?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(|e| ElicitErrorKind::Validation(e.to_string()))?;

    let result = async {
        let comm = TuiCommunicator::with_prompt_rows(6);
        IceCreamWorkflow::new(comm).take_order().await
    }
    .await;

    execute!(io::stdout(), LeaveAlternateScreen).map_err(|e| ElicitErrorKind::Validation(e.to_string()))?;
    disable_raw_mode().map_err(|e| ElicitErrorKind::Validation(e.to_string()))?;

    let order = result?;
    print_order_result(&order.scoop, &order.proof);
    Ok(())
}

/// Agent session: MCP stdio + [`ElicitClient`].
#[instrument]
async fn run_mcp_session() -> ElicitResult<()> {
    let service = ()
        .serve(rmcp::transport::stdio())
        .await
        .expect("Failed to create MCP client");
    let client = ElicitClient::new(Arc::new(service.peer().clone()));
    let order = IceCreamWorkflow::new(client).take_order().await?;
    print_order_result(&order.scoop, &order.proof);
    Ok(())
}

/// Walk the stoplight VSM and show that Yellow → Green is structurally impossible.
#[instrument]
fn demo_stoplight_vsm() {
    let mut state = StoplightState::Green;
    let mut proof = Established::<StoplightConsistent>::assert();

    (state, proof) = advance_to_yellow(state, proof, 30);
    println!("\nStoplight after green≥30s: {state:?}");

    (state, proof) = advance_to_red(state, proof, 5);
    println!("Stoplight after yellow≥5s:  {state:?}");

    (state, _) = advance_to_green(state, proof);
    println!("Stoplight after red clears: {state:?}");

    let stuck = StoplightState::Yellow;
    let (still_yellow, _) = advance_to_green(stuck, Established::assert());
    println!(
        "advance_to_green from Yellow → {still_yellow:?}  (no transition — pass-through)"
    );
}

#[instrument]
fn print_order_result(scoop: &contracts::Scoop, proof: &Established<ScoopComplete>) {
    let _ = proof;
    println!("\n✓ Order complete: one scoop of {}", scoop.flavor());
}
