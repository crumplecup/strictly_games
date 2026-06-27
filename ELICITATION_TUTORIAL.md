# Elicitation Tutorial: From Bare Enum to Verified State Machine

A soup-to-nuts walkthrough using `IceCreamFlavor` as a running example — the
same steps applied throughout Strictly Games to build walled gardens where
invalid agent actions are structurally impossible.

---

## What Is Elicitation?

An MCP agent calling your tools can pass anything, in any order. Elicitation
inverts that: instead of the agent pushing arbitrary values into your functions,
your types *pull* values from the agent through a verified round-trip.

```text
Traditional (push):                 Elicitation (pull):
──────────────────                  ───────────────────
Agent → tool_call("Strawberry")     your_type.elicit(&server).await?
                                        ↑ shows the agent exactly what
validate(arg)?  // hope valid           options exist; agent picks one;
use(value)      // fingers crossed      you get a type-safe value back
```

The rest of the framework layers on top of that baseline:

| Layer | What it adds |
| --- | --- |
| `#[derive(Elicit)]` | Generates `elicit()`, MCP JSON schema, and proof methods |
| Prompt attributes | Controls what the agent/user sees at the call site |
| Style system | Separate prompts for human and agent audiences |
| Contracts (`Prop` / `Established`) | Compile-time proof tokens — validated once, carried forward |
| `kani::Arbitrary` / `#[cfg(kani)]` | Formal verification with Kani model checker |
| `VerifiedStateMachine` | Proof-carrying state machines with auto-generated harnesses |

By the end of this tutorial all eight steps will be in place for a working
ice cream order workflow, and the VSM section will demonstrate the same
pattern on a traffic stoplight — a canonical example of unreachable states.

---

## Runnable Example

The tutorial code lives as a **Cargo example** inside `strictly_games` — not
a separate crate. Ice cream and stoplights are teaching examples only.

```text
crates/strictly_games/examples/elicitation_tutorial/
├── main.rs       # TUI or MCP stdio entry point
├── flavor.rs     # Steps 1–5
├── contracts.rs  # Step 6
├── workflow.rs   # IceCreamWorkflow pipeline
└── stoplight.rs  # Step 8 VSM
```

**Interactive TUI** (human at the keyboard):

```bash
cargo run -p strictly_games --example elicitation_tutorial
# or: just example-elicitation-tutorial
```

**MCP stdio** (external agent via Claude CLI or similar):

```bash
cargo run -p strictly_games --example elicitation_tutorial -- --mcp
# or: just example-elicitation-tutorial-mcp
```

The TUI path uses `TuiCommunicator` from `elicit_ratatui` — the same machinery
as the Strictly Games lobby. The MCP path uses `ElicitClient` over stdio, matching
the `enums` example in the elicitation crate.

This example builds on **stable Rust** only. Nightly is not required to run it.

---

## Prerequisites

Add these dependencies to your crate's `Cargo.toml`. A workspace using Strictly
Games already has them; a standalone crate needs them explicitly:

```toml
[dependencies]
elicitation  = { version = "0.12", features = [] }
serde        = { version = "1",    features = ["derive"] }
schemars     = { version = "0.8" }
tracing      = { version = "0.1" }
```

Editor: elicitation is at version 0.11.1, and rmcp is also a required dependency.

The runnable example (`just example-elicitation-tutorial`) needs **stable Rust**
only. Step 7 (Kani formal verification) is separate: it runs in `strictly_proofs`
via `just verify-*` recipes, which opt into a nightly toolchain for Kani and
Creusot. The workspace default in `rust-toolchain.toml` is stable.

---

## Step 1: The Bare Enum

Start with the smallest thing that compiles — a plain Rust enum.

```rust
// src/flavor.rs

/// A flavor of ice cream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IceCreamFlavor {
    /// Classic vanilla bean.
    Vanilla,
    /// Rich dark chocolate.
    Chocolate,
    /// Fresh strawberry.
    Strawberry,
}
```

Nothing unusual here. Document the variants — that documentation propagates
into generated prompts and MCP schemas later.

---

## Step 2: Add Serde and JsonSchema

The elicitation derive generates an MCP JSON schema so that agents know the
exact shape of every type they encounter. Both `Serialize`/`Deserialize` and
`JsonSchema` are required for that to work.

```rust
// src/flavor.rs

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A flavor of ice cream.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum IceCreamFlavor {
    /// Classic vanilla bean.
    Vanilla,
    /// Rich dark chocolate.
    Chocolate,
    /// Fresh strawberry.
    Strawberry,
}
```

With just these two derives the type can already be serialized into a JSON
schema. Run `cargo check` to confirm nothing is broken before moving on.

---

## Step 3: Add `#[derive(Elicit)]`

Adding `Elicit` to the derive list is the moment the type becomes agent-native.

```rust
// src/flavor.rs

use elicitation::Elicit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A flavor of ice cream.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,                // ← new
)]
pub enum IceCreamFlavor {
    /// Classic vanilla bean.
    Vanilla,
    /// Rich dark chocolate.
    Chocolate,
    /// Fresh strawberry.
    Strawberry,
}
```

### What the derive generates

For a unit-variant enum, `#[derive(Elicit)]` automatically implements:

- **`Select`** — the "choose from a finite set" paradigm. Options are the
  variants; labels are their names (`"Vanilla"`, `"Chocolate"`, `"Strawberry"`).
- **`Prompt`** — prompt text derived from the type name (`"IceCreamFlavor"`
  by default until you add a `#[prompt("...")]` attribute in Step 4).
- **`Elicitation`** — the async `elicit()` method that drives the MCP round-trip.
- **`ElicitComplete`** — a **marker supertrait** that gates generic bounds.
  `T: ElicitComplete` is the single bound any generic container needs to
  confirm that every framework trait has been implemented for `T`. Writing
  `impl ElicitComplete for MyType {}` won't compile until all required traits
  are satisfied — it functions as a living checklist.
- **`ElicitSpec`** — an agent-browsable contract describing the type.

Editor: ElicitComplete is a back of house concern. Explaining it does not add value to the user. If you need to call out specific traits from the list, like you did with ElicitSpec, then do so, but spare the user from this sort of deep dive for no cause.

### Using it

In an `async` function that has an `ElicitCommunicator`:

```rust
use elicitation::{ElicitCommunicator, ElicitResult, Elicitation};

async fn take_order(server: &ElicitCommunicator) -> ElicitResult<()> {
    let flavor = IceCreamFlavor::elicit(server).await?;
    println!("Order: {flavor:?}");
    Ok(())
}
```

`Elicitation` is exported from the crate root (`use elicitation::Elicitation`).

Editor: Obviously from the code. I had to correct you but this does not need its own call out in the tutorial.

Calling `.elicit()` drives a round-trip through MCP: the agent sees a selection
tool with exactly three choices. It cannot produce `IceCreamFlavor::Mango` —
that variant does not exist in the schema. The walled garden has its first wall.

---

## Step 4: Prompt Attributes

The auto-generated prompt is the type name (`"IceCreamFlavor"`). Add
`#[prompt("...")]` to control exactly what the agent sees.

```rust
// src/flavor.rs

use elicitation::Elicit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A flavor of ice cream.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
)]
#[prompt("Pick a flavor:")]          // ← new: enum-level prompt
pub enum IceCreamFlavor {
    /// Classic vanilla bean.
    Vanilla,
    /// Rich dark chocolate.
    Chocolate,
    /// Fresh strawberry.
    Strawberry,
}
```

The `#[prompt("...")]` attribute on the enum sets the overall question.
The variant labels are still derived from the variant names (and their
`///` doc comments are included in the MCP schema description).

When the enum is a field inside a struct, the prompt goes on the field.
Note that the struct also needs `Serialize`, `Deserialize`, and `JsonSchema`
— `rmcp`, the library elicitation is built on, requires them for MCP tool
registration:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Elicit)]
pub struct OrderForm {
    #[prompt("Pick a flavor:")]
    pub flavor: IceCreamFlavor,

    #[prompt("How many scoops?")]
    pub scoops: u8,
}
```

The `Survey` paradigm drives each field in sequence — the agent cannot
skip `flavor` and jump to `scoops`.

---

## Step 5: The Style System

A terse machine prompt is fine for an agent; a warm question is better for
a human. The style system lets you embed both audiences in the same type.

**Enums** (`IceCreamFlavor`) get a default-only style enum today — the derive
generates `IceCreamFlavorStyle { Default }`. Use a single `#[prompt(...)]` on
the enum for the select menu text.

**Structs** (`OrderForm`) support named styles. Without any `#[prompt]`
attribute the derive generates a single `Default` style and `Prompt::prompt()`
returns `None` (the framework then falls back to the type name). Once you add
named styles the derive creates a companion `OrderFormElicitStyle` enum with one
variant per style name.

Add style-specific prompts to the struct fields with the `style = "..."` argument:

```rust
// src/contracts.rs  (or wherever OrderForm lives)

/// Order form elicited field-by-field.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Elicit)]
pub struct OrderForm {
    #[prompt("Pick a flavor:")]
    #[prompt("Which flavor would you like today?", style = "human")]
    #[prompt("flavor: vanilla|chocolate|strawberry", style = "agent")]
    pub flavor: IceCreamFlavor,

    #[prompt("How many scoops?")]
    pub scoops: u8,
}
```

### What gets generated

The derive creates an `OrderFormElicitStyle` enum alongside the struct:

```rust
// auto-generated — do not write this by hand
pub enum OrderFormElicitStyle {
    Default,
    Human,
    Agent,
}
```

### Using it at the call site

```rust
use elicitation::{Elicitation, ElicitCommunicator};

let order = OrderForm::elicit(
    &server.with_style(OrderFormElicitStyle::Human),
).await?;
```

Switch the style at the boundary where you know your audience. The game
logic does not change; only the prompt text does.

In Strictly Games, the TUI uses `Human` style, MCP agent sessions use
`Agent` style, and automated tests use the `Default` style to keep
expected strings stable.

---

## Step 6: Proof-Carrying Contracts

Up to this point, `elicit()` returns a validated `IceCreamFlavor` — but nothing
prevents you from passing an *unelicited* flavor into downstream functions.
Contracts close that gap.

### Define a proposition

A **proposition** is a zero-sized type that represents a claim:

```rust
// src/contracts.rs

use elicitation::contracts::Established;

/// Proposition: a flavor was chosen through the legitimate elicitation path.
#[derive(elicitation::Prop)]
pub struct FlavorChosen;
```

### Write a validator

The validator is the single point of trust. It checks the condition and, if
satisfied, calls `Established::assert()` to issue the proof token:

```rust
// src/contracts.rs  (continued)

use crate::{FlavorError, IceCreamFlavor};

/// Validates that a flavor was legitimately elicited and issues a proof token.
///
/// This is the *only* place `FlavorChosen` proofs are created.
pub fn validate_flavor_chosen(
    flavor: &IceCreamFlavor,
) -> Result<Established<FlavorChosen>, FlavorError> {
    // In a real system this might check a session token or audit flag.
    // Receiving any valid variant is sufficient for ice cream.
    let _ = flavor;
    Ok(Established::assert())
}
```

### The token exchange: `ProvableFrom`

Receiving a proof and just holding it as `_proof` doesn't advance the
contract chain. The real pattern is a **token exchange**: consume the incoming
proof as a credential, and issue a *new* proof for the next step.

Use `ProvableFrom` to declare the exchange relationship, and
`Established::prove(&credential)` to perform it:

```rust
// src/contracts.rs  (continued)

use elicitation::contracts::ProvableFrom;

/// Proposition: a scoop has been legitimately prepared.
#[derive(elicitation::Prop)]
pub struct ScoopComplete;

/// Evidence bundle proving a scoop was prepared from a legitimate flavor choice.
///
/// Assembling this bundle forces the caller to produce `Established<FlavorChosen>`,
/// which in turn can only come from `validate_flavor_chosen`.
pub struct ScoopEvidence {
    /// Proof that the flavor was legitimately elicited.
    pub flavor_chosen: Established<FlavorChosen>,
}

/// Declare that `ScoopEvidence` is a valid credential for `ScoopComplete`.
impl ProvableFrom<ScoopEvidence> for ScoopComplete {}

/// Scoops ice cream and issues proof that the scoop is legitimate.
///
/// Returns the scoop *and* `Established<ScoopComplete>` — the latter acts
/// as a sidecar proof that downstream steps can require.
pub fn scoop(
    flavor: IceCreamFlavor,
    flavor_proof: Established<FlavorChosen>,
) -> (Scoop, Established<ScoopComplete>) {
    let evidence = ScoopEvidence { flavor_chosen: flavor_proof };
    let sidecar = Established::<ScoopComplete>::prove(&evidence);
    (Scoop::new(flavor), sidecar)
}
```

The proof token travels forward with the value. Any subsequent step that
requires `Established<ScoopComplete>` is guaranteed to have passed through
`scoop` — which in turn required `Established<FlavorChosen>` — which in turn
required `validate_flavor_chosen`. The full elicitation path is baked into
the types.

### Composing proofs

When two preconditions must both hold, combine proofs with `both()`:

```rust
use elicitation::contracts::{And, both};

#[derive(elicitation::Prop)]
pub struct ToppingChosen;

/// Requires both flavor AND topping to have been legitimately chosen.
pub fn assemble_sundae(
    flavor: IceCreamFlavor,
    topping: Topping,
    _proof: Established<And<FlavorChosen, ToppingChosen>>,
) -> Sundae {
    Sundae { flavor, topping }
}

// At the call site:
let flavor_proof = validate_flavor_chosen(&flavor)?;
let topping_proof = validate_topping_chosen(&topping)?;
let combined = both(flavor_proof, topping_proof);  // And<FlavorChosen, ToppingChosen>
assemble_sundae(flavor, topping, combined);
```

Editor: When combining proofs, this is a good time to talk about evidence bundles, and show how proof exchanges work, again with the sidecar pattern. So you exchange the proof param for ValidSundae token, using ProvableFrom.

### What this costs at runtime

Nothing. `Established<P>` is `PhantomData<P>` — it disappears after
monomorphization. The compiler verifies the proof chain; the binary carries
no trace of it.

---

## Step 7: Kani Formal Verification

Contracts give you compile-time proof-carrying. Kani gives you **model-checked
exhaustive proof** that the code is correct for *all* possible inputs.

### Add `kani::Arbitrary`

For a leaf enum whose variants carry no data, adding `kani::Arbitrary` is
the only Kani derive you need — it lets the model checker generate every
possible variant symbolically:

```rust
// src/flavor.rs  (additions only — not visible in non-kani builds)

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash,
    Serialize, Deserialize, JsonSchema, Elicit,
)]
#[prompt("Pick a flavor:")]
#[prompt("Which flavor would you like today?", style = "human")]
#[prompt("flavor: vanilla|chocolate|strawberry",  style = "agent")]
#[cfg_attr(kani, derive(kani::Arbitrary))]   // ← new
pub enum IceCreamFlavor { /* ... */ }
```

`kani::Arbitrary` is sufficient for flat enums — there is no depth to
explore because there are no nested fields.

`elicitation::KaniCompose` comes into play for **struct types or
state-machine state enums** where each field must be independently
varied. It generates depth-bounded constructors (`kani_depth0`,
`kani_depth1`, …) so the outer type can be symbolically constructed
from its fields. The VSM state enum in Step 8 derives it; `IceCreamFlavor`
as a leaf type does not need it.

### Proof generation is automatic

In Strictly Games you **never write Kani harnesses by hand**. The `elicitation`
CLI reads your VSM source and generates the entire proofs crate:

```bash
# Regenerate crates/strictly_proofs from all three game VSM crates:
just generate-all
```

Under the hood that runs:

```bash
elicitation generate proof-crate \
    --crate-path crates/strictly_tictactoe \
    --crate-path crates/strictly_blackjack \
    --crate-path crates/strictly_craps \
    --crate-name strictly_proofs \
    --out crates/strictly_proofs
```

Editor: note we probably have to update this to pick up our example crate as well, so our example generates proofs.

For your own crate:

```bash
elicitation generate kani \
    --crate-path crates/my_vsm/src \
    --out crates/my_proofs/src/kani/generated
```

Editor: I am not sure this syntax is correct, we pretty much always generate a separate crate.

Run the generated harnesses:

```bash
just verify-kani-tracked        # runs all harnesses, writes CSV
just verify-kani-vsm            # VSM transition harnesses only
```

Every `#[formal_method]` transition you write in Step 8 gets a
`<transition>__kani_closure` harness auto-generated for free. The
invariant is checked across all reachable states and all symbolic inputs.

---

## Step 8: Verified State Machine

A **Verified State Machine (VSM)** is the culmination: a state enum whose
valid transitions are proof-carrying functions that the `elicitation` CLI
can model-check exhaustively.

The clearest illustration of what VSMs guarantee is a traffic stoplight. The
legal cycle is `Green → Yellow → Red → Green`. Going from `Yellow` directly
to `Green` is not just invalid at runtime — it is **structurally impossible**
because no transition exists for it.

### Define the state enum

```rust
// src/stoplight.rs

use elicitation::{Elicit, KaniCompose, KaniVariantState, VerifiedStateMachine, formal_method};
use elicitation::contracts::Established;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// State of a traffic stoplight.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
    KaniVariantState,   // per-variant Kani constructors (used by generated harnesses)
    KaniCompose,        // depth-bounded constructors for embedding this type in larger structs
)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
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
```

### Define the invariant and machine

```rust
// src/stoplight.rs  (continued)

use elicitation::VerifiedWorkflow;

/// Proposition: the stoplight is in a consistent phase of its cycle.
#[derive(elicitation::Prop)]
pub struct LightConsistent;

impl VerifiedWorkflow for LightConsistent {}

/// The verified state machine — zero data, just the proof wiring.
#[derive(VerifiedStateMachine)]
#[vsm(transitions = [advance_to_yellow, advance_to_red, advance_to_green])]
pub struct StoplightMachine;
```

### Write the transitions

Each transition receives the current state and the invariant proof, applies
its guard, and returns `(new_state, proof)`. The `#[formal_method]` attribute
wires the proof chain and registers the transition for harness generation:

```rust
// src/stoplight.rs  (continued)

use tracing::instrument;

/// Green → Yellow.  Guard: the green phase lasted at least 30 seconds.
#[formal_method(contracts = [LightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_yellow(
    state: StoplightState,
    proof: Established<LightConsistent>,
    elapsed_seconds: u32,
) -> (StoplightState, Established<LightConsistent>) {
    let StoplightState::Green = state else {
        return (state, proof);   // not Green — pass through unchanged
    };
    if elapsed_seconds < 30 {
        return (StoplightState::Green, proof);   // guard not met
    }
    (StoplightState::Yellow, proof)
}

/// Yellow → Red.  Guard: the yellow phase lasted at least 5 seconds.
#[formal_method(contracts = [LightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_red(
    state: StoplightState,
    proof: Established<LightConsistent>,
    elapsed_seconds: u32,
) -> (StoplightState, Established<LightConsistent>) {
    let StoplightState::Yellow = state else {
        return (state, proof);
    };
    if elapsed_seconds < 5 {
        return (StoplightState::Yellow, proof);
    }
    (StoplightState::Red, proof)
}

/// Red → Green.  No time guard — red clears when the cycle restarts.
#[formal_method(contracts = [LightConsistent])]
#[instrument(skip(proof))]
pub fn advance_to_green(
    state: StoplightState,
    proof: Established<LightConsistent>,
) -> (StoplightState, Established<LightConsistent>) {
    let StoplightState::Red = state else {
        return (state, proof);
    };
    (StoplightState::Green, proof)
}
```

### The state diagram and the impossible transition

```text
          advance_to_yellow(≥30s)
 Green  ─────────────────────────►  Yellow
   ▲                                   │
   │  advance_to_green                 │ advance_to_red(≥5s)
   │                                   ▼
  Red  ◄───────────────────────────  Red
```

An agent interacting with this VSM:

- **Can** call `advance_to_yellow` with `elapsed >= 30` from `Green` → moves to `Yellow`.
- **Can** call `advance_to_red` with `elapsed >= 5` from `Yellow` → moves to `Red`.
- **Can** call `advance_to_green` from `Red` → moves to `Green`.
- **Cannot** reach `Green` directly from `Yellow`. `advance_to_green` pattern-matches
  `StoplightState::Red` on the first line; any other state is passed through
  unchanged. There is no `advance_to_yellow_to_green` transition — it does not
  exist in the action space. The agent cannot express it.

The proof token `Established<LightConsistent>` travels with the state at every
step. Kani's generated harnesses verify that every reachable transition
preserves the invariant across all symbolic states and all elapsed-time values.

### Connecting to Strictly Games

The tic-tac-toe, blackjack, and craps VSMs follow the identical pattern:

```text
TicTacToeState: Setup → InProgress → Finished → (restart) → Setup
BlackjackState: Setup → Betting → PlayerTurn → DealerTurn → Finished
CrapsState:     Betting → ComeOut → Point → (resolve) → Betting
```

Each illegal path — placing a bet after the hand ends, taking a player action
during the dealer turn — is structurally absent from the transition table.

---

## Putting It Together: The Elicitation Pipeline

The steps above show how to build the type. Here is how the type connects to
an actual agent interaction using the same pattern as `BlackjackWorkflow` in
Strictly Games.

```rust
// src/workflow.rs

use elicitation::{ElicitCommunicator, ElicitResult, Elicitation};
use tracing::instrument;

use crate::contracts::{ScoopComplete, scoop, validate_flavor_chosen};
use crate::flavor::IceCreamFlavor;

/// Result of a complete ice cream order workflow.
pub struct OrderResult {
    /// The scooped ice cream.
    pub scoop: Scoop,
    /// Proof that the scoop was prepared from a legitimately elicited flavor.
    pub proof: elicitation::contracts::Established<ScoopComplete>,
}

/// Workflow driver for an ice cream order.
///
/// Generic over any `ElicitCommunicator` — the same code drives human TUI
/// sessions and AI agent sessions.
pub struct IceCreamWorkflow<C> {
    communicator: C,
}

impl<C: ElicitCommunicator> IceCreamWorkflow<C> {
    /// Creates a new workflow.
    pub fn new(communicator: C) -> Self {
        Self { communicator }
    }

    /// Run a complete order: elicit flavor, validate, scoop, return proof.
    ///
    /// # Proof chain
    ///
    /// ```text
    /// (nothing) → [elicit] → IceCreamFlavor
    ///                              ↓
    ///                    [validate] → Established<FlavorChosen>
    ///                                          ↓
    ///                             [scoop] → (Scoop, Established<ScoopComplete>)
    /// ```
    #[instrument(skip(self))]
    pub async fn take_order(&self) -> ElicitResult<OrderResult> {
        // ── Step 1: elicit a flavor from whoever is on the other end ─────
        let flavor = IceCreamFlavor::elicit(&self.communicator).await?;

        // ── Step 2: validate and establish the FlavorChosen proof ────────
        let flavor_proof = validate_flavor_chosen(&flavor)
            .map_err(|e| elicitation::ElicitErrorKind::Validation(e.to_string()))?;

        // ── Step 3: scoop — consumes FlavorChosen, issues ScoopComplete ──
        let (s, proof) = scoop(flavor, flavor_proof);

        Ok(OrderResult { scoop: s, proof })
    }
}
```

The same `IceCreamWorkflow` can be driven by a human typing in the TUI,
an AI agent making MCP tool calls, or an automated test using a mock
communicator — the workflow code never changes, only the communicator
passed at construction.

Editor: How does the user wire up communication with an agent, like claude haiku? This "where the rubber meets the road" portion is missing, and I would not know how to plug a real agent into this framework.

One way this manifests in the example code is that you don't actually elicit an agent for the stoplight value at the start of the example, or any of the transitions. Instead, you mint an out of band proof by calling assert in the user code, which is A BAD PATTERN not to be used in examples, it encourages users to abandon the proof minting framework that we have introducted, and is in fact the worst thing we could show them how to do.  Instead, the example should show how we make an agent "play the stoplight game" and how they are forced to follow the transition rules we have laid out

The ice cream flavor example at least elicits from an agent, but it uses TuiCommunicator, which gets no explanation in the tutorial, so we are not really showing users how to communicate with agents, we are showing them 90% of the work, and letting them guess at the last 10%.

---

## How This Maps to Strictly Games

Every game in Strictly Games follows exactly this progression:

| Game | Leaf types | State machine | Proof harnesses |
| --- | --- | --- | --- |
| Tic-Tac-Toe | `Player`, `Square`, `Position` | `TicTacToeState` (Setup → InProgress → Finished) | 130+ |
| Blackjack | `Rank`, `Suit`, `Card`, `Hand`, `BetAmount` | `BlackjackState` (5-phase) | 100+ |
| Craps | `DieFace`, `DiceRoll`, `ComeoutResult` | `CrapsState` (3-phase) | 50+ |

The ice cream / stoplight example touches every layer:

1. **Leaf type** (`IceCreamFlavor`) → corresponds to `Player`, `Rank`, `Suit`
2. **Prompt / style** → used throughout for TUI vs MCP agent sessions
3. **Contracts** (`FlavorChosen`, `ScoopComplete`) → corresponds to `SquareEmpty`, `PlayerTurn`, `NotBust`, `PayoutSettled`
4. **`kani::Arbitrary`** → all leaf enums in the games derive it
5. **VSM** (`StoplightMachine`) → corresponds to `TicTacToeMachine`, `BlackjackMachine`
6. **Workflow** (`IceCreamWorkflow`) → corresponds to `BlackjackWorkflow` in `strictly_server`

### Further reading

| Document | What it covers |
| --- | --- |
| `README.md` | Overview and the five layers of correctness |
| `FORMAL_VERIFICATION.md` | Kani, Verus, Creusot proof architecture |
| `PROOF_ARCHITECTURE.md` | Compositional proof system in depth |
| `KANI_DEV_GUIDE.md` | Running and extending Kani harnesses |
| `crates/strictly_tictactoe/src/types.rs` | Smallest real examples of `#[derive(Elicit)]` |
| `crates/strictly_tictactoe/src/vsm.rs` | Clean, annotated VSM implementation |
| `crates/strictly_blackjack/src/vsm.rs` | Five-phase VSM with richer proof chains |
| `crates/strictly_server/src/games/blackjack/workflow/runner.rs` | Full workflow pipeline |
| `crates/strictly_proofs/` | All harnesses — the proof crate that verifies everything |
