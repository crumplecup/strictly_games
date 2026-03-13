# Critique of the Blackjack Proof-Carrying Workflow

This document reviews the server-side blackjack workflow (`tools.rs`, `runner.rs`, `propositions.rs`) and evaluates whether the implementation delivers the “proof‑carrying contract chain” architecture described in the guide.

Overall: **it’s very close** — about 90% of the way to a fully airtight, proof‑gated workflow. The remaining gaps are fixable with small refactors.

---

## ✔️ What’s Working Extremely Well

### 1. Phase-Gated Tools

Each free function correctly encodes its required precondition proof and returns an appropriate postcondition proof:

- `execute_place_bet`: implicit `True → BetPlaced`
- `execute_play_action`: `BetPlaced → (BetPlaced | PlayerTurnComplete)`
- `execute_dealer_turn`: `PlayerTurnComplete → PayoutSettled`

These represent a clear typestate progression enforced by the compiler rather than by runtime checks.

### 2. Communicator-Agnostic Orchestration

`BlackjackWorkflow<C>` cleanly abstracts over human vs. agent players.  
All game logic is independent of the driver; elicitation is the only boundary.

### 3. Well-Documented Propositions

`BetPlaced`, `PlayerTurnComplete`, and `PayoutSettled` form a clean, readable chain.  
The financial invariants (`BetDeducted`, `settle()`, no double-deduction) are properly described at the type level.

---

## ⚠️ Where the Implementation Falls Short

### A. Dealer Natural Fast Path Breaks the Uniform Gating

In `execute_place_bet()`, the dealer’s turn is executed directly:

```rust
let finished = dt.play_dealer_turn();
```

This bypasses `execute_dealer_turn()`, meaning dealer logic can run **without** the `PlayerTurnComplete` proof.  
That breaks the “every phase must pass a gate” guarantee.

**Fix:** Introduce a dedicated fast-path dealer tool:

```rust
execute_dealer_natural(dt, Established<BetPlaced>)
    → (GameFinished, Established<PayoutSettled>)
```

Or return a `DealerNatural(dt)` variant and make the caller invoke the correct tool explicitly.

---

### B. `PayoutSettled` Documentation Doesn’t Match Implementation

`propositions.rs` claims:

> Established by `execute_dealer_turn` **and by instant-finish paths in `execute_place_bet`.**

But the code never returns `PayoutSettled` from fast paths — it always returns a `BetPlaced` proof.  
This is a mismatch between contract and implementation.

**Fix:** Ensure the fast path returns a settlement token, e.g.:

```rust
PlaceBetOutput::Finished(finished, Established<PayoutSettled>)
```

---

### C. “Recycled” Proofs Are Actually Reminted

`execute_play_action` produces new `Established::assert()` tokens instead of threading the existing one.  
This is fine **only** if proof minting is restricted to these modules.

**Fix:**  
Make `Established::assert()` private (or in a sealed trait) so proofs cannot be forged externally.  
Expose only `#[cfg(test)]` constructors to tests.

---

### D. Final Settlement Proof Not Propagated to Observability Layer

`execute_dealer_turn()` returns `Established<PayoutSettled>` as promised, but:

- `runner.rs` discards it
- No proof event is logged
- No unified trace is produced for human/agent observability

Given the framework’s emphasis on “traces for an AI reader,” this loses valuable telemetry.

**Fix:** Bubble the final proof up into `HandResult` or emit a log event (“PayoutSettled”).

---

### E. Action Model Doesn’t Yet Enforce Micro-Rules

Right now, `BasicAction` → `PlayerAction::new()` handles actions, but:

- Split legality
- Double-down legality
- Surrender and insurance states

…are **not encoded at the proposition level**.

If you want the “walled garden” to extend beyond macro-phase ordering into _per-action legality_, adding propositions like:

- `CanSplit`
- `CanDouble`
- `InsuranceOffered`

…would prevent invalid move generation for both humans and agents.

---

## 🛠️ Recommended Patches

### 1. Add a Dealer-Natural Tool

```rust
pub fn execute_dealer_natural(
    dealer_turn: GameDealerTurn,
    _pre: Established<BetPlaced>,
) -> (GameFinished, Established<PayoutSettled>);
```

### 2. Modify `PlaceBetOutput` to Require Proper Settlement

```rust
enum PlaceBetOutput {
    PlayerTurn(GamePlayerTurn),
    DealerNatural(GameDealerTurn),
    Finished(GameFinished, Established<PayoutSettled>),
}
```

### 3. Restrict Proof Construction

- Keep `Established::assert()` private to the module.
- Provide test-only constructors behind `#[cfg(test)]`.

### 4. Propagate Settlement Proof Into Workflow Output

Add to `HandResult`:

```rust
pub final_proof: Established<PayoutSettled>,
```

Or emit structured events: `"PayoutSettled"`.

---

## 🎯 Final Verdict

You’re extremely close to achieving a **fully type-driven, structurally walled garden**:

- No skipped phases
- No illegal agent moves
- No representable invalid states
- Observability grounded in proof transitions
