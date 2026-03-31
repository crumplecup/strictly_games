# Blackjack as an Elicitation Showcase

> If you want to understand a framework, find the most interesting thing it can express.

Blackjack is the flagship demonstration of the [Elicitation Framework](https://github.com/crumplecup/elicitation) in Strictly Games. It shows what the framework is actually *for*: not just walling off invalid moves, but expressing an entire multi-phase game as a **proof-carrying contract chain** where the compiler guarantees you cannot reach any phase without having legally passed through every prior phase first.

---

## The Core Idea

Elicitation's `Established<P>` type is a zero-cost proof token. It has no runtime representation — it is purely a compile-time certificate that proposition `P` holds at this point in the program. The type system enforces that you cannot construct one without going through the function that asserts it.

Blackjack maps naturally onto this:

```text
True → execute_place_bet → BetPlaced → execute_play_action (loop) → PlayerTurnComplete
                        ↓                                                    ↓
               PayoutSettled                              execute_dealer_turn → PayoutSettled
             (fast-finish path)
```

Each arrow is a function. Each label is an `Established<P>` token. You cannot call `execute_dealer_turn` without a `Established<PlayerTurnComplete>`. You cannot call `execute_play_action` without an `Established<BetPlaced>`. These are not runtime checks — they are type errors.

---

## The Three Propositions

```rust
/// Proposition: a bet has been placed and initial cards dealt.
pub struct BetPlaced;
impl Prop for BetPlaced {}

/// Proposition: the player's turn is complete (stood, bust, or blackjack).
pub struct PlayerTurnComplete;
impl Prop for PlayerTurnComplete {}

/// Proposition: the hand's payout has been correctly settled.
///
/// Re-exported from strictly_blackjack — carrying this token proves
/// BankrollLedger::settle() ran with a valid BetDeducted proof.
pub use strictly_blackjack::PayoutSettled;
```

These are zero-byte markers at runtime — their entire purpose is to appear as
type parameters on `Established<P>`, making certain function signatures
uncompilable unless called in the right order.

`PayoutSettled` carries stronger semantics than a plain "hand resolved" marker:
it *proves the financial settlement ran*. Because `BetDeducted` is a linear
token (non-`Copy`, consumed by `settle()`), carrying `PayoutSettled` proves
the payout was computed exactly once.

---

## The Proof-Carrying Functions

### Step 1: Place Bet

```rust
pub fn execute_place_bet(
    betting: GameBetting,
    bet: u64,
) -> Result<PlaceBetOutput, ActionError>
```

`PlaceBetOutput` is an enum, not a plain tuple, because two outcomes are
possible immediately after the deal:

```rust
pub enum PlaceBetOutput {
    /// Normal path — player acts next.
    PlayerTurn(GamePlayerTurn, Established<BetPlaced>),
    /// Fast-finish — player natural, dealer natural, or push on both naturals.
    Finished(GameFinished, Established<PayoutSettled>),
}
```

On the normal path, you receive `Established<BetPlaced>` to continue with.
On fast-finish paths (a player blackjack, a dealer blackjack, or both), the
hand is already settled: you receive `Established<PayoutSettled>` proving that
`BankrollLedger::settle()` already ran. The payout proof is threaded out of
`place_bet` so callers always hold the settlement receipt at the end of the
hand, regardless of which path was taken.

### Step 2: Player Actions (loop)

```rust
pub fn execute_play_action(
    player_turn: GamePlayerTurn,
    action: BasicAction,
    _pre: Established<BetPlaced>,          // ← must hold this to call me
) -> Result<PlayActionResult, ActionError>
```

The `_pre: Established<BetPlaced>` parameter is the proof requirement. The compiler will not let you call this function unless you have a `BetPlaced` token in hand. The underscore prefix is intentional — the value is never inspected at runtime. Its job is to exist in the type signature.

When the player's turn ends, the function returns `Established<PlayerTurnComplete>` inside `PlayActionResult::Complete`. While the hand is still going, it recycles `Established<BetPlaced>` inside `PlayActionResult::InProgress`, allowing the loop to continue.

### Step 3: Dealer Turn

```rust
pub fn execute_dealer_turn(
    dealer_turn: GameDealerTurn,
    _pre: Established<PlayerTurnComplete>,  // ← must hold this to call me
) -> (GameFinished, Established<PayoutSettled>)
```

This function is infallible — once you hold `PlayerTurnComplete`, the dealer
turn always produces a resolved game. The returned `Established<PayoutSettled>`
token is the proof receipt: it can only exist because `BankrollLedger::settle()`
ran inside `play_dealer_turn`, consuming a `BetDeducted` token that was itself
produced by `BankrollLedger::debit()`. The financial sub-chain is:

```text
BankrollLedger::debit() → Established<BetDeducted>
                                     ↓  (consumed exactly once)
BankrollLedger::settle() → Established<PayoutSettled>
```

Holding `PayoutSettled` at the end of a hand is a compile-time proof that
money moved exactly once in each direction.

---

## What the Compiler Enforces

You cannot write this:

```rust
// ERROR: missing Established<BetPlaced>
let result = execute_play_action(player_turn, BasicAction::Hit, ???);
```

You cannot write this:

```rust
// ERROR: missing Established<PlayerTurnComplete>
let (finished, _) = execute_dealer_turn(dealer_turn, ???);
```

You cannot write this:

```rust
// ERROR: wrong proposition type
let bet_proof: Established<BetPlaced> = /* ... */;
let (finished, _) = execute_dealer_turn(dealer_turn, bet_proof);
//                                                   ^^^^^^^^^^ expected PlayerTurnComplete
```

These are not panics. They are not runtime errors. They are compile-time type errors. A blackjack game that skips the player turn and goes straight to the dealer is **not a valid Rust program**.

The fast-finish path has the same guarantee in the other direction: you cannot
call `execute_dealer_turn` *after* `execute_place_bet` already returned
`PlaceBetOutput::Finished` — you have no `PlayerTurnComplete` to pass in
because that state was never reached.  The type system makes the shortcut
visible and prevents misuse.

---

## The Single Interface for Human and Agent

The most important property of this design: the game logic is **completely independent of who is playing**.

```rust
pub struct BlackjackWorkflow<C: ElicitCommunicator> {
    communicator: C,
}
```

`BlackjackWorkflow<TuiCommunicator>` is a human playing in the terminal.  
`BlackjackWorkflow<C: ElicitCommunicator>` works equally for any communicator — including an MCP agent implementation.

The proof chain is identical. The `execute_*` functions are identical. The only difference is what happens when `elicit()` is called — either a human types a response in raw mode, or an AI agent responds to a structured prompt over a tool-call protocol.

This is the **walled garden pattern** applied to an entire game session rather than a single move. The agent cannot skip the bet phase, cannot take actions out of order, cannot trigger the dealer turn without completing the player turn. These constraints are not enforced by the agent's instructions — they are enforced by the Rust type system.

### Style-Aware Elicitation

When `BasicAction::elicit(&communicator)` is called, the `ElicitationStyle` associated type on the communicator controls *how* the prompt is rendered. A human player sees a terminal menu. An AI agent receives a structured JSON schema describing the legal moves. The elicitation framework generates the prompt from the type — the game code never branches on "am I talking to a human or an agent?"

---

## The TUI: Rendering + Proofs Together

The TUI game loop in `blackjack.rs` wires the proof chain directly into the ratatui rendering cycle:

```rust
// ── execute_place_bet (True → BetPlaced or PayoutSettled) ─────────────
match execute_place_bet(betting, bet)? {
    PlaceBetOutput::Finished(finished, _settled) => {
        // Fast-finish: player natural, dealer natural, or push.
        // _settled: Established<PayoutSettled> — settlement already ran.
        return Ok(finished);
    }
    PlaceBetOutput::PlayerTurn(player_turn, bet_proof) => {
        // Normal path — continue with the player action loop.
        event_log.push(GameEvent::proof("BetPlaced"));

        // ── player action loop (BetPlaced → PlayerTurnComplete) ───────────────
        let mut current_proof: Established<BetPlaced> = bet_proof;
        loop {
            render_blackjack(terminal, DisplayPhase::PlayerTurn { &state }, ...)?;
            let action = BasicAction::elicit(comm).await?;

            match execute_play_action(state, action, current_proof)? {
                PlayActionResult::InProgress(next, proof) => {
                    state = next;
                    current_proof = proof;  // BetPlaced recycled for next iteration
                }
                PlayActionResult::Complete(output, player_done_proof) => {
                    // ── execute_dealer_turn (PlayerTurnComplete → PayoutSettled) ──
                    let (finished, _settled) = execute_dealer_turn(dt, player_done_proof);
                    // _settled: Established<PayoutSettled> — settlement proved
                    return Ok(finished);
                }
            }
        }
    }
}
```

The `event_log` is fed to `TypestateGraphWidget` — the right-hand panel in the split-view TUI. Every proof establishment (`BetPlaced`, `PlayerTurnComplete`, `PayoutSettled`) appears as an event in the live graph, so you can watch the contract chain advance in real time as the hand plays out.

### Multi-Round with Preserved Proof Integrity

The session loop plays multiple hands back-to-back:

```rust
loop {
    let Some((hand_result, outcome)) = run_single_hand(...).await? else {
        return Ok(Abandoned);  // player quit
    };
    // ... show result, prompt "play again?"
    if bankroll == 0 { break; }
    if !prompt_play_again(terminal, bankroll).await? { break; }
}
```

Each iteration of this loop runs a *fresh* proof chain — a new `BetPlaced` token for the new hand, distinct from any token from a prior hand. There is no shared mutable proof state between hands. The type system makes it impossible to accidentally reuse a `PlayerTurnComplete` from one hand as the precondition for the dealer turn of the next.

---

## Why This Architecture Matters

### For Game Integrity

Classical game servers validate moves by checking state in a database or in-memory structure. If the validation is missing or has a bug, invalid states are possible. In this architecture, invalid states are unrepresentable in the type system. A bug that would cause the dealer to play before the player would be a *compile error*, not a logic bug.

### For Agent Orchestration

AI agents that play blackjack through MCP cannot hallucinate moves they haven't been offered. They cannot call tools out of order — the tool for the dealer phase does not exist in their context until the player turn is complete. The "walled garden" is structural, not instructional.

### For Observability

Every proof transition is a named, structured event. The `event_log` is a first-class citizen of the game loop — not an afterthought. When something goes wrong, the trace shows exactly which propositions were established and in what order. This is the principle described in CLAUDE.md: *write traces for an AI reader*.

### For Testing

Because the game logic lives in pure functions (`execute_place_bet`, etc.) that take and return value types, they are trivially testable without spinning up a TUI, a terminal, or an MCP server. The proof tokens can be constructed with `Established::assert()` in tests to inject at any phase.

---

## Card Generation with `Shoe`

Blackjack showcases **stateful generation** — a fundamentally different pattern
from the stateless dice generators in craps.

### The Problem with Manual Card Management

Traditional blackjack implementations pass `&mut Deck` through every function
that deals cards. This creates borrow-checker friction in multi-player
scenarios where multiple seats and the dealer all draw from the same deck.

### The Elicitation Solution: `Shoe` as a `Generator`

`Shoe` implements `elicitation::Generator<Target = Option<Card>>`, using
`AtomicUsize` interior mutability so `generate(&self)` works without `&mut`:

```rust
pub struct Shoe {
    cards: Vec<Card>,
    dealt: AtomicUsize,  // Interior mutability for &self generate
}

impl Generator for Shoe {
    type Target = Option<Card>;

    fn generate(&self) -> Option<Card> {
        let d = self.dealt.fetch_add(1, Ordering::Relaxed);
        if d < self.cards.len() {
            Some(self.cards[d])
        } else {
            None
        }
    }
}
```

### Building Blocks: `#[derive(Rand)]` on Rank and Suit

Both `Rank` and `Suit` derive `Rand` (feature-gated), giving them
`random_generator(seed)` methods for uniform selection. `Shoe` composes these
to build full decks, then shuffles using a seeded RNG from `elicitation_rand`.

### Ergonomic Win in Multi-Player Code

The `&self` signature dramatically simplifies the multi-player table:

```rust
// Before (Deck): borrow checker friction
fn hit(&mut self, deck: &mut Deck) -> Result<(), ActionError>

// After (Shoe): clean shared reference
fn hit(&mut self, shoe: &Shoe) -> Result<(), ActionError>
```

Multiple seats share `&Shoe` without fighting over mutable borrows.

### Dice vs Cards: Two Generator Patterns

| Aspect       | Craps Dice              | Blackjack Cards          |
| ------------ | ----------------------- | ------------------------ |
| Pattern      | i.i.d. (independent)    | Without replacement      |
| State        | Stateless generator     | Stateful (pool shrinks)  |
| Interior mut | `RefCell<StdRng>` (rand)| `AtomicUsize` (counter)  |
| Derive       | `#[derive(Rand)]`       | Manual `Generator` impl  |
| Target       | `DiceRoll`              | `Option<Card>`           |
| Exhaustion   | Never                   | Returns `None` when empty|

---

## The Elicitation Stack in One Diagram

```text
┌─────────────────────────────────────────────────────────────┐
│                    run_blackjack_session                      │
│  (multi-round loop — renders TUI between each elicitation)   │
├─────────────────────────────────────────────────────────────┤
│                      run_single_hand                          │
│                                                              │
│  execute_place_bet ──→ PlaceBetOutput                        │
│         │               /         \                          │
│         │    PlayerTurn(BetPlaced)  Finished(PayoutSettled)  │
│         │       │              (fast-finish: natural, etc.)  │
│         │  execute_play_action (loop)                        │
│         │       │                                            │
│         │  Established<PlayerTurnComplete>                   │
│         │       │                                            │
│         └─────→ execute_dealer_turn                          │
│                       │                                      │
│               Established<PayoutSettled>                     │
│    (proves BankrollLedger::settle ran exactly once)          │
├─────────────────────────────────────────────────────────────┤
│                    ElicitCommunicator                         │
│                                                              │
│   TuiCommunicator          (any ElicitCommunicator)            │
│   (crossterm raw mode)     (e.g. MCP tool calls)              │
│                                                              │
│   Same elicit() calls. Different prompt rendering.           │
│   Same proof chain. Different communication channel.         │
└─────────────────────────────────────────────────────────────┘
```

This is what the elicitation framework enables: a game where the rules are the types, the moves are the proofs, and humans and agents are interchangeable passengers through the same walled garden.
