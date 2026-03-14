# Blackjack Proof‑Carrying Typestate — Implementation Plan

**Objective:** Express blackjack as a _proof‑carrying contract chain_ where invalid states and transitions are unrepresentable, financial movement is linear and audited at the type level, and human/agent orchestration is neutral. This plan specifies the exact refactors, APIs, proofs, tests, and migration steps to converge on the correct architecture.

---

## 0) Scope & Outcomes

- **Phases**: `GameSetup → GameBetting → GamePlayerTurn → GameDealerTurn → GameFinished`
- **Phase boundary proofs**: `Established<BetPlaced>`, `Established<PlayerTurnComplete>`
- **Action lattice**: targeted propositions per action (`CanSplit`, `CanDouble`, `CanSurrender`, plus shared `CorrectHandIndex`, `InRange`, `NotBust`)
- **Finance**: vectorized `settle_many(outcomes, bets, Established<BetDeducted>) → Established<PayoutSettled>`
- **Observability**: instrumented transitions + structured proof events
- **Verification**: Kani scenario harnesses (fast‑finish, normal path, multi‑hand), action legality harnesses
- **Ergonomics**: error taxonomy improvements, helpers to avoid footguns

Deliverables are organized as a PR stack with acceptance criteria and test coverage per step.

---

## 1) Typestate Encapsulation (Illegal State Construction Impossible)

### Tasks

- Make **all phase struct fields private** (`pub(crate)` or `pub(super)` where needed).
- Remove any non‑transition constructors; expose **only** functions that produce the next phase.
- Mark proof tokens with `#[must_use]` via newtype wrappers to discourage dropping proofs unintentionally.
- Ensure `GameSetup → GameBetting → ...` can only be created via exported transitions.

### Acceptance Criteria

- No external module can construct `GameBetting`, `GamePlayerTurn`, `GameDealerTurn`, or `GameFinished` without using provided transitions.
- Rust compiler rejects any attempt to bypass transitions (private fields + module boundaries).
- CI passes with updated visibility.

---

## 2) Phase‑Boundary Proofs (Minimal, Meaningful Markers)

### Definitions

- `BetPlaced`: “initial debit and initial deal completed for _this round_.”
- `PlayerTurnComplete`: “all player hands resolved (bust/stand/blackjack) for _this round_.”

### API Changes

- `GameBetting::place_bet(bet) → Result<PlaceBetOutput, ActionError>`
  ```rust
  pub enum PlaceBetOutput {
      PlayerTurn(GamePlayerTurn, Established<BetPlaced>),
      Finished(GameFinished, Established<PayoutSettled>),
  }
  ```
- `GamePlayerTurn::take_action(self, action, pre: Established<BetPlaced>, legal: Established<...>) → Result<PlayerActionResult, ActionError>`
  ```rust
  pub enum PlayerActionResult {
      InProgress(GamePlayerTurn, Established<BetPlaced>),         // proof recycled
      Complete(GameDealerTurn, Established<PlayerTurnComplete>),  // produced at final hand completion
  }
  ```
- `GameDealerTurn::play_dealer_turn(pre: Established<PlayerTurnComplete>) → (GameFinished, Established<PayoutSettled>)`

### Acceptance Criteria

- Compile‑time requirement: dealer turn cannot be invoked without `PlayerTurnComplete`.
- Player actions cannot be taken without `BetPlaced`.
- Proof tokens are **zero‑cost** (PhantomData) with no runtime overhead.

---

## 3) Action Lattice (Small, Composable Propositions)

### Propositions

- Shared: `CorrectHandIndex`, `InRange`, `NotBust`
- Action‑specific: `CanSplit`, `CanDouble`, `CanSurrender`

### Validation APIs

- `validate_legal_hit(pt, action) → Established<And<CorrectHandIndex, InRange>>`
- `validate_legal_stand(...) → Established<And<CorrectHandIndex, InRange>>`
- `validate_legal_split(...) → Established<And<LegalHit, CanSplit>>`
- `validate_legal_double(...) → Established<And<LegalHit, CanDouble>>`
- `validate_legal_surrender(...) → Established<And<LegalHit, CanSurrender>>`

### Execution APIs

- `execute_action(self, action, pre: Established<BetPlaced>, legal: Established<...>) → PlayerActionResult`

### Acceptance Criteria

- Each action is **gated** by its specific proposition set.
- Refusal to compile if caller lacks the correct `Established<...>` proof.
- Unit tests: happy‑path + negative tests per action eligibility.

---

## 4) Multi‑Hand Settlement (Single Debit, Aggregate Returns)

### Ledger API

```rust
impl BankrollLedger {
    pub fn settle_many(
        self,
        outcomes: &[Outcome],
        bets: &[u64],
        token: Established<BetDeducted>,
    ) -> (u64, Established<PayoutSettled>) {
        assert_eq!(outcomes.len(), bets.len());
        let total_gross = outcomes.iter().zip(bets)
            .map(|(o, b)| o.gross_return(*b))
            .sum::<u64>();
        let final_bankroll = self.post_bet_balance() + total_gross;
        (final_bankroll, Established::assert())
    }
}
```

### Integration

- `GameDealerTurn::resolve()` calls `settle_many` with **all** per‑hand outcomes and bets.
- Remove single‑outcome settlement; ensure atomic consumption of `BetDeducted`.

### Acceptance Criteria

- Split scenarios correctly aggregate returns across hands.
- Token linearity preserved: `BetDeducted` consumed **exactly once** by `settle_many`.
- Kani proof covers vectorized arithmetic.

---

## 5) PlayerTurn Progression & Completion

### Mechanics

- Completion rule: a hand is complete if **bust** or **Stand**; blackjack on deal may short‑circuit to fast‑finish.
- `advance_hand()`:
  - If more hands remain → recycle `Established<BetPlaced>`
  - If last hand just completed → produce `Established<PlayerTurnComplete>` and transition to `GameDealerTurn`

### Acceptance Criteria

- `PlayerTurnComplete` produced only when **all** hands finished.
- State machine cannot bypass final production of `PlayerTurnComplete`.
- Unit tests across single and multi‑hand flows.

---

## 6) Elicitation & Serialization Hygiene

### Tasks

- Ensure `BasicAction` and phase types derive `Elicit`/`JsonSchema` where appropriate.
- Proof tokens **never serialize**; redact in `Debug`.
- Provide `action_on_current(&self, BasicAction) → PlayerAction` helper to avoid index mistakes.

### Acceptance Criteria

- Human and agent flows share the same `elicit()` calls.
- No proof material leaks through serialization; logs redact tokens.

---

## 7) Observability (Traces for an AI Reader)

### Tasks

- `#[instrument]` on all transition functions; include IDs and summary state.
- Emit structured events: `ProofEstablished::BetPlaced`, `ProofEstablished::PlayerTurnComplete`, `ProofEstablished::PayoutSettled`.
- TUI/graph consumes these events to visualize the proof chain.

### Acceptance Criteria

- Transition logs include proof milestones without exposing token internals.
- Typestate graph updates live through the entire hand.

---

## 8) Formal Verification (Kani) — New Scenario & Vector Proofs

### Add Harnesses

1. **Fast‑finish: player natural**

   - Deterministic deck → player blackjack (and optionally dealer blackjack for push)
   - Assert `place_bet` returns `Finished(..., Established<PayoutSettled>)`
   - Assert final bankroll arithmetic per outcome

2. **Normal path: Hit → Hit → Stand → Dealer**

   - Deterministic deck ensures non‑natural start
   - Assert player loop recycles `BetPlaced`
   - Assert dealer requires `PlayerTurnComplete` and returns `Established<PayoutSettled)`

3. **Multi‑hand settlement**

   - Symbolic `outcomes[]`, `bets[]`, `len` equality
   - Assert final bankroll = `post_bet_balance + Σ gross_return(bet_i)`
   - Token consumed once

4. **Action legality**
   - `CanSplit` iff two cards of same rank; `CanDouble` iff exactly two cards; `CanSurrender` per policy; negative cases reject

### Acceptance Criteria

- All new harnesses pass under bounded unwinding.
- CI target includes Kani jobs for these harnesses.

---

## 9) Error Taxonomy & Ergonomics

### Changes

- Add `WrongHandTurn { expected, got }` separate from `InvalidHandIndex(usize)`.
- Implement `GamePlayerTurn::action_on_current(BasicAction) → PlayerAction` to eliminate manual index plumbing.

### Acceptance Criteria

- Error messages distinguish out‑of‑range vs wrong‑turn index.
- Fewer footguns in call sites; code reads cleanly.

---

## 10) Migration Plan

### Steps

- Introduce new proofs and APIs.
- Update internal call sites incrementally: phase constructors → proof‑gated methods.
- Deprecate legacy single‑outcome settlement; provide shims with compile‑time warnings.

### Acceptance Criteria

- No breaking changes to external consumers unless they rely on non‑typestate construction.
- Clear deprecation notes; migration guide in repository.

---

## 11) Performance & Safety Notes

- Proof tokens are zero‑size; no runtime cost.
- Vectorized settlement iterates `O(n)` over hands; acceptable size `n` << 10.
- No unsafe code; leverage ownership and privacy for safety.
- Deck operations remain amortized `O(1)` per deal.

---

## 12) PR Stack & Timeline (Indicative)

1. **Encapsulation & visibility** (private fields, constructors) — 1–2 days
2. **Phase proofs (BetPlaced, PlayerTurnComplete)** — 1 day
3. **Action lattice + validators** — 2–3 days
4. **Vector settlement + resolve integration** — 1–2 days
5. **Kani harnesses (3 scenario + multi‑hand + legality)** — 2–3 days
6. **Observability & event stream** — 1 day
7. **Error taxonomy & ergonomics** — 0.5 day
8. **Migration cleanups & docs** — 1 day

Parallelize where possible (observability, errors can be concurrent).

---

## 13) Success Criteria

- **Compilation gates** enforce phase order and per‑action legality.
- **Single debit, aggregate settlement**, token consumed exactly once.
- **Scenario proofs** demonstrate both execution paths; **vector proofs** validate multi‑hand correctness.
- **Human/agent neutrality** preserved; **observability** exposes proof chain; **errors** are precise.
