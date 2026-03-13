# Proofs Critique (Focused on Gaps, Weaknesses, and Missing Reasoning)

This critique covers the shortcomings and structural gaps in:

- `blackjack_compositional.rs`
- `blackjack_invariants.rs`
- `bankroll_financial.rs`

The focus is strictly on what is missing, under-specified, or inadequately justified.

---

# 1. `blackjack_compositional.rs`

## Issues

### 1.1. Proofs are shallow: no behavioral properties

All proofs generated here (`Rank::kani_proof()`, etc.) only confirm structural well-formedness. They do _not_ verify any semantic properties of the types.

- For example, `Outcome::kani_proof()` confirms enum reachability but does not connect to payout semantics, which appear later in the financial proofs.  
  There is no cross-proof linkage.

### 1.2. Composition claim is too informal

The module asserts that verifying `Rank` and `Suit` verifies `Card` “by composition.”  
However, nothing is shown about `Card`'s _behavior_ (e.g., value mapping), only that its fields are valid.

### 1.3. Entire-hierarchy proof is a tautology

`verify_blackjack_legos()` ends with `assert!(true, ...)`, which adds no constraint or verification.  
This proof contributes nothing except a narrative claim.

### 1.4. No negative examples or failure-mode analysis

There are no counterexample-driven checks verifying that invalid constructions are impossible or rejected.  
Structural proofs alone leave behavioral correctness unspecified.

---

# 2. `blackjack_invariants.rs`

## Issues

### 2.1. Deck uniqueness is unproven

The code verifies:

- deck size = 52
- deal reduces remaining by 1
- exhausted deck => `None`

But it does **not** prove:

- all 52 dealt cards are unique
- the same card cannot be dealt twice

This allows undetected duplication bugs since count invariants alone do not imply uniqueness.

### 2.2. Value semantics are only partially covered

The test:

```rust
assert!(value >= 1 && value <= 11)
```

confirms bounds but not correctness.

Missing invariants:

- J/Q/K must be 10
- Ace hard value must be 1
- No rank may map to invalid values even if the function unrolls incorrectly

Your later tests indirectly imply these constraints but never assert them parametrically.

### 2.3. Soft/hard value relationship is under‑specified

Several proofs check soft totals and hard totals, but they only assert:

- soft ≤ 21
- soft ≥ hard

Missing explicit rule:

- If a soft value exists, `soft == hard + 10`

This mathematical relation is essential to blackjack semantics but is not stated.

### 2.4. Ace handling is incomplete

Only a few fixed examples are provided (e.g., `A,6` and `A,10,5`).  
Missing cases include:

- multiple aces (e.g., `A,A`, `A,A,9`)
- interaction of soft and hard values with 2–4 aces
- transitions where soft collapses to hard after adding additional cards

### 2.5. Hand bounds proof relies on saturating arithmetic

`hand_value_bounds()` asserts:

```rust
assert!(value.hard() <= 127)
```

This limit is derived from implementation details (saturating_add) rather than rules.  
The proof does not articulate:

- the combinatorial max hard total from up to 7 cards
- the intended invariant independent of saturating arithmetic

### 2.6 Split logic is only partially verified

Current tests verify:

- `can_split()` when ranks match
- not when they differ
- not with invalid card counts

Missing verification:

- That split is only legal with _exactly two cards_ of same rank (checked), but does not enforce further gameplay constraints like:

  - splits of aces behaving differently
  - prohibitions on re-splitting
  - restrictions after certain actions

Even if out of scope for now, the proof suite does not indicate what subset of rules is being proven.

### 2.7. No invariants for hand ordering or stability

`Hand::new` with arbitrary cards is used as a black box; no checks confirm:

- hand order does not affect value
- no mutation of input
- no invalid internal states in `HandValue`

---

# 3. `bankroll_financial.rs`

## Issues

### 3.1. Rounding behavior insufficiently specified

For Blackjack and Surrender, flooring behavior is asserted indirectly via integer division:

```rust
(bet * 3)/2
bet/2
```

Missing:

- explicit tests for small odd bets: 1, 3, 5, 101
- guaranteed alignment with real payout policies (floor vs round-to-even vs table-dependent variants)

Currently the rounding rule is implicit, not documented or proven.

### 3.2. Overflow constraints repeated and fragile

Multiple proofs manually include lines like:

```rust
kani::assume(bet <= u64::MAX / 3);
kani::assume(bankroll <= u64::MAX - bet);
```

Issues:

- Repetition increases the chance of missing correct overflow guards in future changes.
- The assumptions are not centrally defined and could diverge from the code’s actual arithmetic constraints.

A single overflow guard function or macro would reduce risk.

### 3.3. Debit preconditions are not fully enumerated

`debit` assumes:

- bet > 0
- bet <= bankroll

Missing:

- explicit rejection of pathological inputs (e.g., u64::MAX with bet=1 and bankroll underflow scenarios)
- postconditions confirming ledger values remain within representable bounds for all valid inputs

### 3.4. No temporal sequencing properties

There are no proofs confirming that:

- `settle()` cannot be invoked without a prior valid debit
- the token representing `BetDeducted` cannot be forged (proof uses rely on Rust privacy, not formal verification)

### 3.5. No multi-hand or split-hand proofs

The settlement system is single-hand only.  
When splits are later introduced, there should be proofs verifying:

- independent `BetDeducted` tokens per hand
- no mixing of tokens between hands
- settlement of multiple hands cannot cause integer inconsistencies

### 3.6. Token linearity proof lacks compile-fail enforcement

The explanatory comment shows an example of double-use:

```rust
ledger.settle(..., token);
ledger.settle(..., token); // ERROR
```

but no actual compile-fail (`trybuild`) test exists.  
This leaves structural correctness untested through CI.

### 3.7. Round-trip identities cover only linear cases

Current round-trip proofs verify:

- win → bankroll + bet
- push → bankroll
- loss → bankroll - bet
- surrender → bankroll - (bet/2)

Missing:

- composite round-trips for multiple outcomes over sequential hands
- invariants for cumulative error or drift over repeated operations

---

# Final Summary of Needed Fixes

- Add **uniqueness proofs** for cards in a deck.
- Make **value semantics explicit**: face-card mapping, ace handling, soft/hard exact relation.
- Replace saturating-arithmetic bounds with **rule-derived bounds**.
- Centralize and systematically test **overflow guard assumptions**.
- Add **edge-case rounding tests** for all payout types.
- Add **compile-fail tests** showing token non-duplicability.
- Extend proofs to cover **multi-hand scenarios** (splits, re-splits).
- Strengthen compositional module with links to behavioral proofs and remove tautological harnesses.

These adjustments would produce a far more complete and reliable deductive environment for training agents and for ensuring the correctness of the blackjack implementation.

# Constructive Critique of KANI_LAWYERS_GUIDE.md

_(Based strictly on the proof files reviewed: blackjack_compositional.rs, blackjack_invariants.rs, bankroll_financial.rs)_

## 1. Incorrect or Overstated Claims About Blackjack Proof Coverage

### 1.1. Guide claims the blackjack–21 rule is fully _biconditional_

The guide states:

> “Property: is_blackjack(h) ⟺ h = 2 ∧ value(h) = 21” [1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)  
> But the proof files only include:

- A _one-directional_ implication: if `hand.is_blackjack()` then the hand has 2 cards and totals 21.
- A few positive/negative examples (Ace+Ten, Ace+King, 3-card 21), but **no general proof of the converse**.  
  [2](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/bankroll_financial.rs)

**Recommended change:** Rewrite the guide claim as a _one-way_ implication unless you add a general converse harness.

---

## 2. Misrepresentation of What `kani_proof()` Guarantees

### 2.1. Guide implies Outcome → payout mapping is proven in compositional proofs

The guide says Outcome “maps directly to payout rules” and this mapping is part of type-system verification. [1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)  
But in the actual files:

- compositional proofs only verify _structural exhaustiveness_
- payout correctness is proven separately in `bankroll_financial.rs`  
  [3](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/blackjack_invariants.rs)[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)

**Recommended change:** Move all behavioral/payout semantics to the financial section; leave the compositional section strictly structural.

---

## 3. Missing or Incorrect Harness Names

### 3.1. Many harness names in the guide do not exist

Example mismatches:

| Guide Name                       | Actual Harness in Code        |
| -------------------------------- | ----------------------------- |
| `verify_new_deck_size`           | `deck_has_52_cards`           |
| `verify_deal_decrements_deck`    | `deal_reduces_remaining`      |
| `verify_empty_deck_returns_none` | `exhausted_deck_returns_none` |

[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)[2](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/bankroll_financial.rs)

**Recommended change:** Either update the guide to reflect the real harness names or explicitly state that names are simplified for readability.

---

## 4. Missing Proofs That the Guide Describes as “Proved”

### 4.1. Face-card value mapping is not proven

Guide states that Ten/Jack/Queen/King “all count as 10”. [1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)  
However `blackjack_invariants.rs` only proves value is in the range 1..=11 and uses fixed examples, not a parametric enumerated proof.  
[2](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/bankroll_financial.rs)

**Recommended change:** Add a harness asserting `J/Q/K → 10` for all suits, or soften guide wording to “demonstrated by examples”.

---

### 4.2. Soft/hard equality rule is not proven

The guide claims that “soft” totals behave with Ace=11 semantics.  
But soft value invariants are only tested indirectly (soft ≤21, soft ≥ hard).  
There is **no proof**:

- that `soft == hard + 10` whenever soft exists
- that only one Ace is counted as 11

[2](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/bankroll_financial.rs)

**Recommended change:** Add a dedicated soft/hard coherence harness or amend the guide.

---

### 4.3. Deck uniqueness is absent but implied by “deck integrity”

Guide repeatedly speaks as though all 52 cards behave as a normal deck.  
But no proof verifies **uniqueness** (e.g., no duplicate cards dealt across 52 draws). [2](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/bankroll_financial.rs)

**Recommended change:** Add a uniqueness harness or edit guide to clarify that only count, not uniqueness, is proven.

---

## 5. Compositional Harness Section Overstates What Is Actually Verified

### 5.1. Guide asserts full ecosystem-level verification

It states:

> “Once primitives are verified, Card is verified, Outcome is verified… therefore the entire type system is verified.” [1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)  
> But in `blackjack_compositional.rs`, the “ecosystem proof” ends in:

```rust
assert!(true, "…");
```

which contributes **no composite invariants** beyond prior proofs.  
[3](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/blackjack_invariants.rs)

**Recommended change:** Either remove this rhetorical closure or convert it into a real cross-type property (e.g., constructing any Card from any Rank×Suit → value ∈ 1..=11).

---

## 6. Financial Section Mostly Accurate, But Missing Important Caveats

### 6.1. Guide glosses over explicit overflow assumptions

The financial proofs rely on outcome-specific `kani::assume` bounds (e.g., `bet <= MAX/3`, `post <= MAX - bet`).  
These are not described in the guide.  
[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)

**Recommended change:** Document that payouts are proven within explicit overflow-safe domains.

---

### 6.2. No mention that odd-bet flooring is tested only arithmetically

Guide claims blackjack and surrender rounding are fully tested, but the current proofs test formulas symbolically, not concrete cases like bet=1,3,5,101.  
[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)

**Recommended change:** Add explicit odd-bet examples or weaken the guide’s language.

---

## 7. Structural Guarantees: Missing Compile-Fail Enforcement

### 7.1. Guide claims double-settlement is “eliminated at compile time”

The claim is correct in principle, but the repo **does not include any compile-fail test** (e.g., `trybuild`) verifying that misuse of the proof token fails CI, even though such tests are easy to add.  
[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)

**Recommended change:** Add a compile-fail harness demonstrating the move-error scenario, or clarify that the guarantee is theoretical (compiler-level), not currently enforced in tests.

---

## 8. Scope Misalignment

### 8.1. Guide claims “54 harnesses” but the 3 modules provided do not match this total

Those 54 include tic-tac-toe proofs and other modules outside the provided files.  
[1](https://grantspassoregon-my.sharepoint.com/personal/erose_grantspassoregon_gov/Documents/Microsoft%20Copilot%20Chat%20Files/KANI_LAWYERS_GUIDE.md)

**Recommended change:** Clarify that blackjack occupies only a subset of the total proof count.

---

# Summary of Required Fixes to Align Guide With Proof Files

1. Correct blackjack biconditional claim to one-way implication.
2. Correct or add proofs for face-card mapping and soft/hard exact relation.
3. Add a deck-uniqueness proof or adjust language.
4. Remove or fix the vacuous assert in the compositional harness.
5. Align harness names with real code or disclaim simplification.
6. Document overflow assumptions in financial proofs.
7. Add compile-fail tests for token misuse or soften language.
8. Clarify that total proof count covers additional modules beyond blackjack.

Applying these changes will make the guide precisely match the reviewed proofs and avoid overstating guarantees.
