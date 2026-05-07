# Formal Verification for Games — A Plain-Language Guide

*How the strictly_games engine proves correctness using Verified State Machines
and three independent mathematical tools.*

---

## Who this is for

You don't need to write Rust to read this guide. If you've ever asked *"how do
I know this software actually does what it claims?"* — and you want an answer
more satisfying than "we tested it a lot" — keep reading.

---

## The problem with testing

Traditional software testing works by picking specific inputs and checking the
output. A casino's risk team might run a thousand test hands and confirm payouts
look right. But a thousand tests can never prove correctness for *every* possible
input. There are billions of combinations of bankroll, bet size, and game state.
A bug can hide in a corner no tester ever reached.

**Formal verification is different.** Instead of testing samples, it reasons
over *all possible inputs simultaneously* using mathematical logic. When a formal
proof reports PASS, it is not saying "we checked a lot of cases." It is saying:

> *This property holds for every input in the stated range, with no exceptions.*

---

## The primary architecture: Verified State Machines

The games engine is built around a concept called a **Verified State Machine**
(VSM). This is the most important thing to understand about how correctness is
guaranteed.

### What a state machine is

A state machine is a formal model that says:

- The system is always in exactly one **state** (e.g., `Betting`, `PlayerTurn`,
  `DealerTurn`, `Finished`)
- Only defined **transitions** move the system from one state to another
- Each transition has defined inputs, and produces a defined output

This is not a vague diagram — it is enforced in code. The Rust type system makes
it a compile error to invoke a transition from the wrong state. A betting
operation cannot be called after the player has already acted. An impossible
state sequence is not merely prohibited by a runtime check; it does not compile.

### What an invariant is

Each state machine declares an **invariant** — a property that must be true in
every state, at every moment. Think of it as the machine's constitution.

For example, the blackjack invariant says:

```
In the Betting state:   bankroll > 0
In the PlayerTurn state: current_hand_index < number_of_hands
In the Finished state:  outcomes.len() == bets.len()
```

In craps:

```
In the Setup state:             num_seats > 0
In any active state:            shooter_idx < bankrolls.len()
```

In tic-tac-toe:

```
In the Setup state:       every square is empty
In the InProgress state:  history.len() ≤ 9
In the Finished state:    history.len() ≥ 5
```

These invariants are **formal, mathematical specifications** — not prose
descriptions. They are expressed as code that the verification tools can
reason about directly.

### How proofs are automatically derived

Here is the key architectural fact: **a developer annotates a function as a
state machine transition, and the verification harnesses are generated
automatically.**

```rust
#[formal_method(contracts = [BlackjackConsistent])]
pub async fn bj_start_betting(
    state: BlackjackState,
    proof: Established<BlackjackConsistent>,
    initial_bankroll: u64,
    bankroll_proof: Established<BankrollPositive>,
) -> (BlackjackState, Established<BlackjackConsistent>) {
    // production implementation
}
```

The `#[formal_method(contracts = [BlackjackConsistent])]` annotation tells the
`VerifiedStateMachine` derive macro: *every time this function is called with a
consistent state, it must return a consistent state.* The macro then
**automatically generates** a verification harness for each of the three proof
tools (Kani, Creusot, Verus) — no hand-written proof code required.

Adding a new transition automatically adds a new proof obligation. The developer
cannot forget to write the proof: the framework demands it.

---

## The trifecta: three independent verifiers

Each VSM invariant is verified by **three completely independent mathematical
tools**, each using a different verification methodology. A property that passes
all three has been checked by three separate reasoning engines with no shared
code paths.

| Tool | Method | What it checks |
|------|--------|---------------|
| **Kani** | Bounded model checking (CBMC/Z3) | Memory safety, no panics, arithmetic correctness at all depths |
| **Creusot** | Deductive proof via WhyML + Alt-Ergo | Unbounded functional correctness, loop invariants |
| **Verus** | Unbounded SMT via Z3 | Rich contracts, ghost state, type invariants |

All three must pass for a transition to be considered verified. The three tools
catch different classes of bugs:

- Kani excels at finding concrete counter-examples (it produces the exact input
  that breaks a property when one exists)
- Creusot excels at proving properties over unbounded loops and recursive
  structures
- Verus excels at ghost-state reasoning and contracts expressed close to the
  mathematical specification

---

## VSM proof counts (current)

| Machine | Kani VSM harnesses | Creusot proofs | Verus proofs | Status |
|---------|--------------------|----------------|--------------|--------|
| Blackjack | 5 transitions + 1 marker | 5 contracts | 11 verified | ✅ All pass |
| Craps | 5 transitions + 1 marker | 5 contracts | 8 verified | ✅ All pass |
| Tic-tac-toe | 3 transitions + 1 marker | 3 contracts | 9 verified | ✅ All pass |
| **Total VSM** | **16** | **13** | **28** | **✅ 57/57** |

The Kani VSM harnesses use `proof_for_contract` — they verify that the
production async transition functions satisfy their annotated contracts, by
having the SMT solver check all reachable states up to depth 2.

---

## Supplementary proofs: game-logic detail

In addition to the VSM invariant proofs, the engine carries a suite of
**supplementary harnesses** that prove specific game-logic properties in
detail. These predate the VSM methodology and will gradually be subsumed as
the VSM invariants are made richer, but they provide additional assurance today.

### Blackjack game logic (Kani, 53 harnesses)

Grouped by concern:

**Type system soundness** — Every fundamental type (Rank, Suit, Card, Outcome,
BankrollLedger) is compositionally verified. The type system makes impossible
states structurally unrepresentable.

**Deck and hand correctness** — Every new deck has exactly 52 cards. Dealing
reduces the count by exactly 1. An empty deck returns `None` rather than
panicking. Every card has a value between 1 and 11. The deck contains no
duplicate cards. Soft/hard hand totals are computed correctly.

**Blackjack detection** — `is_blackjack(h) ⟺ |h| = 2 ∧ value(h) = 21`.
Proven as a true biconditional — both directions, both harnesses.

**Financial settlement** — Every payout formula is proven arithmetically
exact, for all valid `u64` inputs:

| Outcome | Property proven |
|---------|----------------|
| Loss | `final = bankroll − bet` |
| Surrender | `final = bankroll − ⌈bet / 2⌉` |
| Push | `final = bankroll` |
| Win | `final = bankroll + bet` |
| Blackjack | `final = bankroll + ⌊bet × 1.5⌋` |

**Double-deduction impossibility** — The `Established<BetDeducted>` proof
token is *not Copy*: it can be moved into `settle` exactly once. A second call
with the same token is a compile error. Kani additionally proves that `settle`
itself is purely additive — it never subtracts from `post_bet_balance`.

**Workflow integration** — Six end-to-end scenario harnesses trace the full
call chain from bet placement through card dealing through player actions
through dealer resolution through settlement, confirming correct `Established`
token flow at each step.

### Craps (Kani, 42 harnesses)

Financial settlement for all craps payout outcomes, multi-seat bankroll
accounting, dice roll semantics, and come-out / point-phase transition
correctness.

### Tic-tac-toe (Kani, 25 harnesses)

Board position arithmetic, move legality, win condition detection, and
player-turn alternation.

### Shared game invariants (Kani, 14 harnesses)

Cross-game type properties: board position index bijection, player opponent
involution, position roundtrip identity.

### TUI breakpoints (Kani, 12 harnesses)

UI rendering preconditions and terminal-layout arithmetic.

### Foundation proofs (Kani, 5 harnesses — generated)

Auto-generated from the VSM bootstrap layer; confirm basic well-formedness of
the type hierarchy before any transitions run.

---

## How `proof_for_contract` works (technical)

The VSM harnesses use Kani's `proof_for_contract` mechanism. Each harness:

1. Constructs a **symbolic state** using `kani_depth2()` — a state with all
   fields set to arbitrary values up to depth 2 in the type tree.
2. **Assumes** the invariant holds on this symbolic state (the pre-condition).
3. Constructs symbolic inputs for the transition (bet amounts, dice rolls, etc.).
4. **Calls the production function** — no mocks, no stubs. The actual async
   transition logic is what Kani analyses.
5. The **contract annotation** on the function provides the post-condition.
   Kani verifies that for every symbolic input satisfying the pre-condition,
   the post-condition holds on the output.

```rust
// AUTO-GENERATED harness (simplified)
#[kani::proof_for_contract(bj_place_bet)]
fn bj_place_bet__kani_closure() {
    // Symbolic pre-state: any BlackjackState, all fields symbolic
    let state = BlackjackState::kani_depth2();
    kani::assume(blackjack_consistent(&state)); // pre-condition
    // Symbolic inputs
    let bet: u64 = kani::any();
    // Call the production code
    let _result = bj_place_bet(state, proof, bet);
    // Post-condition checked via the contract annotation on bj_place_bet
}
```

---

## Reading the proof token pattern

The `Established<T>` type is worth explaining because it appears throughout
the API and is the mechanism that makes several properties hold *structurally*
rather than at runtime.

```text
Established<BetDeducted>
    ↑ can only be created inside BankrollLedger::debit()
    ↑ is not Copy — moves exactly once
    ↑ settle() requires one as input and consumes it

Result: no code path can reach settle() without passing through debit().
        No code path can settle twice. This is not a runtime check.
        A program that tries to settle twice will not compile.
```

The same pattern is used at the VSM level:

```text
Established<BlackjackConsistent>
    ↑ can only be created by asserting the invariant holds
    ↑ passed through every transition in the chain
    ↑ every transition requires one as input and returns one as output

Result: a game session that somehow reaches PlayerTurn with an
        inconsistent state is not merely unlikely — it cannot be
        constructed without forging a proof token, which requires
        bypassing the invariant check.
```

---

## Assumptions and scope

Formal proofs are explicit about what they trust:

- **Rust's ownership model** — the compiler's borrow checker and move
  semantics are trusted as correct. They are independently verified by the
  Rust project.
- **u64 arithmetic** — overflow preconditions are stated explicitly as
  `kani::assume` bounds. Arithmetic outside those bounds is not claimed.
- **The SMT solvers** (Z3, Alt-Ergo, CaDiCaL) — industry-standard tools
  with their own formal correctness arguments and decades of production use.
- **`Established::assert()`** — the proof token constructor is trusted to
  produce exactly one valid token per call. Its implementation is auditable
  in the elicitation framework source.

What is *not* in scope:

- **Shuffle fairness** — randomness of the deck shuffle is a separate
  statistical property.
- **Network or persistence layer** — the proofs cover in-memory game logic.
- **House rules** — the engine proves *mathematical consistency* of the rules
  it implements, not that those rules match any particular jurisdiction's
  requirements.

---

## Summary

| Layer | Description | Count | Status |
|-------|-------------|-------|--------|
| VSM invariant proofs (Kani) | Auto-generated; every transition × every machine | 16 | ✅ |
| VSM invariant proofs (Creusot) | Independent deductive backend | 13 | ✅ |
| VSM invariant proofs (Verus) | Independent SMT backend | 28 | ✅ |
| Supplementary game-logic proofs (Kani) | Payout arithmetic, deck, hand values, scenarios | 151 | ✅ |
| **Total** | | **208** | **208/208** |

The VSM proofs are the primary claim. For every game, for every transition,
starting from any state that satisfies the invariant, the output satisfies the
invariant. This is proven by three independent tools.

The supplementary proofs provide additional granular assurance for payout
arithmetic, hand-value semantics, and workflow integration.

For the financial core — the question of whether a player receives exactly the
right amount of money for each outcome — the answer is not "we think so" or
"testing shows it." The answer is:

**It is mathematically proven, three times over, by independent tools.**

---

*VSM proofs: `crates/strictly_proofs/src/kani_proofs/generated/`*
*Supplementary proofs: `crates/strictly_proofs/src/kani_proofs/`*

```bash
just verify-kani-vsm              # VSM harnesses only (16 harnesses)
just verify-kani-tracked          # All Kani harnesses with CSV tracking
just verify-kani-resume           # Resume after interruption
just verify-kani-summary          # Print pass/fail totals
just verify-kani-vsm-summary      # VSM-only summary
```
