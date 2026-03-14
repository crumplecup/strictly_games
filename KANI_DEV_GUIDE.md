# Kani Proof Engineering Guide

*For agents and developers writing new Kani harnesses in this codebase.*

This document distills hard-won lessons from the Vec→array migration and
`scenario_bankroll_conservation` non-termination debugging session.  Read
this before touching any file in `crates/strictly_proofs/`.

---

## The Golden Rule

> **Kani auto-determines loop bounds only when the bound is a compile-time
> constant.**  Every other loop will unwind forever.

CBMC (the solver backing Kani) must statically know the maximum number of
times each loop can execute.  If it cannot, it keeps unrolling indefinitely
and the process never terminates.

---

## The Three Loop Patterns That Kill Kani

### ❌ Pattern 1 — `while` loop

```rust
// CBMC cannot determine how many times this runs.
while self.dealer_hand.value().best() < 17 {
    self.dealer_hand.add_card(self.deck.deal().unwrap());
}
```

### ❌ Pattern 2 — `for` over a runtime field

```rust
// self.num_hands is not known at model-check time.
for i in 0..self.num_hands {
    // ...
}
```

### ❌ Pattern 3 — `for` over a dynamic slice

```rust
// self.len is a runtime value; the slice bound is not static.
for card in &self.cards[..self.len] {
    // ...
}
```

---

## The One Correct Pattern

Replace every dynamic loop with a **constant bound + early `break`**:

```rust
// ✅ CBMC sees MAX_HAND_CARDS (= 11) and unrolls exactly 11 times.
for i in 0..MAX_HAND_CARDS {
    if i >= self.len {
        break;        // runtime guard — fine, CBMC handles this
    }
    // work on self.cards[i]
}
```

The constant bound must be a `pub const usize` defined in the domain crate,
not a literal.  Literals work too, but named constants survive refactoring and
make the intent obvious.

### Real examples from this codebase

| Location | Before (broken) | After (correct) |
|---|---|---|
| `hand.rs` `Hand::value()` | `for card in &self.cards[..self.len]` | `for i in 0..MAX_HAND_CARDS { if i >= self.len { break; } }` |
| `typestate.rs` `play_dealer_turn` | `while dealer < 17 { … }` | `for _ in 0..MAX_HAND_CARDS { if dealer >= 17 { break; } }` |
| `typestate.rs` `resolve` | `for i in 0..self.num_hands` | `for i in 0..MAX_PLAYER_HANDS { if i >= self.num_hands { break; } }` |

---

## The `Vec` Antipattern

`Vec<T>` has no static size bound.  Kani treats any loop over a `Vec` as
potentially infinite and unwinds indefinitely.

**Rule: no `Vec` in types that are reachable from any Kani harness.**

Replace with a fixed-size array + length counter:

```rust
// ❌ BAD
struct Hand {
    cards: Vec<Card>,
}

// ✅ GOOD
pub const MAX_HAND_CARDS: usize = 11;

struct Hand {
    cards: [Card; MAX_HAND_CARDS],
    len: usize,
}
```

Choose the constant to be the tightest provably-correct bound:

| Constant | Value | Justification |
|---|---|---|
| `MAX_DECK_CARDS` | 52 | One standard deck, 4 suits × 13 ranks |
| `MAX_HAND_CARDS` | 11 | Longest non-bust hand: 4A + 4×2 + 3×3 = 21 |
| `MAX_PLAYER_HANDS` | 4 | 1 initial hand + up to 3 splits |

---

## The `#[kani::unwind(N)]` Antipattern

Never add `#[kani::unwind(N)]` to a harness.  It is a band-aid over an
unbounded loop in the model — it does not fix the problem, it just caps the
damage.  The correct fix is always to make the loop bound statically visible
to CBMC.

```rust
// ❌ Hiding the problem
#[kani::unwind(8)]
#[kani::proof]
fn hand_value_bounds() { … }

// ✅ Fixed root cause — no annotation needed
#[kani::proof]
fn hand_value_bounds() { … }
```

If you find yourself reaching for `#[kani::unwind]`, stop and find the
dynamic loop instead.

---

## Symbolic Harnesses — Unit Invariants

Use `kani::any()` to create fully symbolic inputs.  For arrays of `Copy +
kani::Arbitrary` types, use the array form directly:

```rust
#[kani::proof]
fn hand_value_bounds() {
    // ✅ One call — Kani makes every element independently symbolic.
    let cards: [Card; MAX_HAND_CARDS] = kani::any();
    let hand = Hand::new(&cards[..3]);
    // assert invariants ...
}
```

**Do not** construct symbolic arrays with a fill loop:

```rust
// ❌ The loop itself needs an unwind annotation — defeats the purpose.
let mut cards = [Card::default(); 7];
let n: usize = kani::any();
kani::assume(n <= 7);
for i in 0..n {
    cards[i] = kani::any();   // loop bound n is dynamic
}
```

### Deriving `kani::Arbitrary`

Types used in `kani::any()` calls need `#[cfg_attr(kani, derive(kani::Arbitrary))]`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Card {
    rank: Rank,
    suit: Suit,
}
```

This only activates under `--cfg kani`, so it has zero cost in normal builds.

---

## Concrete Harnesses — Scenario Integration

Scenario harnesses prove end-to-end workflow properties.  Use
`Deck::new_ordered` with a literal `&[Card]` slice — no symbolic inputs, no
`kani::any()`:

```rust
#[kani::proof]
fn scenario_normal_stand() {
    // Deck layout: p1, d1, p2, d2, then remaining cards.
    // Player gets cards[0] and cards[2]; dealer gets cards[1] and cards[3].
    let betting = betting_with_deck(&[
        Card::new(Rank::Ten,   Suit::Spades),   // p1 → 10
        Card::new(Rank::Seven, Suit::Hearts),   // d1 → 7
        Card::new(Rank::Eight, Suit::Clubs),    // p2 → player 18
        Card::new(Rank::Nine,  Suit::Diamonds), // d2 → dealer 16 (will hit)
        Card::new(Rank::Two,   Suit::Hearts),   // dealer draws → 18
    ]);
    // ...
}
```

Concrete decks make the call chain fully determined, so CBMC can evaluate
it without branching.  These harnesses still take 30–140 seconds because of
the proof infrastructure depth, not symbolic branching.

### The `betting_with_deck` helper

```rust
#[cfg(kani)]
fn betting_with_deck(cards: &[Card]) -> GameBetting {
    GameBetting::new(Deck::new_ordered(cards), 1000)
}
```

`Deck::new_ordered` copies the slice into the fixed `[Card; MAX_DECK_CARDS]`
array, padding with defaults.  The slice itself is a literal, so Kani handles
the `copy_from_slice` fine.

---

## When to Use `kani::assume`

`kani::assume` constrains symbolic inputs to the region you actually want to
verify.  Without it you may prove a weaker property (or fail due to cases
your code legitimately rejects).

```rust
#[kani::proof]
fn bet_deducted_from_bankroll() {
    let bankroll: u64 = kani::any();
    let bet: u64 = kani::any();

    // Without this, Kani explores bet > bankroll, which returns Err —
    // an interesting path, but not what this harness is about.
    kani::assume(bet > 0 && bet <= bankroll);

    let (ledger, _proof) = BankrollLedger::debit(bankroll, bet)
        .expect("valid bet");
    assert_eq!(ledger.post_bet_balance(), bankroll - bet);
}
```

Do not over-constrain.  Every `kani::assume` shrinks the space Kani checks;
if you assume too much you may accidentally exclude the bug you're trying to
find.

---

## Two-Tier Harness Architecture

This codebase separates harnesses into two files:

| File | Layer | Inputs | Purpose |
|---|---|---|---|
| `blackjack_invariants.rs` | Unit | `kani::any()` or trivial concrete | Card math, deck tracking, hand value, split rules |
| `blackjack_scenarios.rs` | Integration | Fully concrete `Deck::new_ordered` | End-to-end workflow, financial conservation |

**Add new invariant proofs to `blackjack_invariants.rs`.**  These run in
1–10 seconds and prove properties that hold for *all* inputs.

**Add new scenario proofs to `blackjack_scenarios.rs`.**  These run in
30–140 seconds and prove that specific game paths reach the expected final
state with `Established<PayoutSettled>`.

---

## Running Harnesses

```bash
# Single harness (fastest feedback loop)
cargo kani -p strictly_proofs --harness <harness_name>

# Full suite with CSV output
just verify-kani-tracked

# Resume an interrupted run (skips harnesses already PASS in CSV)
just verify-kani-resume
```

The full suite takes ~20 minutes.  Always run a single harness first to
confirm it passes before kicking off the full run.

---

## Diagnosing Non-Termination

If a harness does not terminate within ~5 minutes, the model is unbounded.
Stop it immediately (`Ctrl-C`) and follow this checklist:

### Step 1 — Find all loops in the call chain

Trace the full call chain reachable from the harness.  For each function
in that chain, look for:

- Any `while` loop
- Any `for x in 0..self.field` or `for x in 0..some_variable`
- Any `for x in &slice[..self.len]` or `for x in iter` over a
  runtime-length collection

### Step 2 — Apply the fix pattern

For every dynamic loop found:

```rust
// Change this:
for i in 0..self.dynamic_field { … }

// To this:
for i in 0..SOME_CONST {
    if i >= self.dynamic_field { break; }
    …
}
```

### Step 3 — Check for `Vec` in reachable types

Any `Vec<T>` in a struct reachable from the harness can cause explosion even
if your loops look correct.  Replace with `[T; N]` + len counter.

### Step 4 — Verify with `cargo check` first

Loop fixes must compile cleanly before retrying Kani.  A quick `cargo check
-p strictly_blackjack` catches mistakes in seconds.

### Step 5 — Retry with a simpler harness

If you're unsure whether the issue is compilation overhead or model
explosion, run the simplest possible scenario (e.g., `scenario_player_natural`
— fast-finish, no dealer loop).  If *that* hangs, the problem is structural
(likely a `Vec` in a shared type).

---

## Checklist for New Harnesses

Before submitting a new harness:

- [ ] No `#[kani::unwind]` annotations
- [ ] No `Vec` in any type reachable from the harness
- [ ] All loops iterate over compile-time constants with `if i >= runtime { break; }`
- [ ] Symbolic inputs use `kani::any::<[T; N]>()` directly, not fill-loops
- [ ] `kani::assume` used to restrict inputs to the property's valid domain
- [ ] Runs to `VERIFICATION:- SUCCESSFUL` within 5 minutes
- [ ] Added to `HARNESSES` list in `justfile` so `verify-kani-tracked` picks it up
- [ ] Module-level doc comment states what is trusted vs. verified ("cloud of assumptions")

---

## Current Harness Inventory

As of the last full run (69/69 PASS):

### Invariant harnesses (`blackjack_invariants.rs`) — ~1–10s each

Card/deck: `deck_has_52_cards`, `deal_reduces_remaining`,
`exhausted_deck_returns_none`, `deck_all_cards_unique`, `card_value_in_range`,
`ace_detection`, `ace_raw_value_is_eleven`

Hand value: `empty_hand_zero_value`, `hand_value_no_aces`,
`hand_value_single_ace_soft`, `hand_value_ace_busts_soft`, `hand_value_bounds`,
`hand_value_equality`, `ace_ace_nine_value`, `ace_ace_ten_soft_collapses`

Blackjack detection: `blackjack_requires_two_cards`, `blackjack_ace_ten`,
`blackjack_ace_king`, `three_card_21_not_blackjack`

Bust detection: `bust_detection`, `no_bust_under_21`, `exactly_21_not_bust`

Split rules: `can_split_matching_ranks`, `cannot_split_different_ranks`,
`cannot_split_wrong_count`, `soft_hard_exact_relation`

### Financial harnesses (`blackjack_invariants.rs`) — ~12–25s each

### Financial harnesses (`blackjack_invariants.rs`) — ~12–25s each

`verify_rank_compositional`, `verify_suit_compositional`,
`verify_card_compositional`, `verify_outcome_compositional`,
`verify_blackjack_legos`, `verify_bankroll_legos`,
`verify_debit_arithmetic`, `verify_debit_overdraft_rejected`,
`verify_debit_zero_bet_rejected`, `verify_no_double_deduction`,
`verify_settle_blackjack`, `verify_settle_loss`, `verify_settle_push`,
`verify_settle_surrender`, `verify_settle_win`, `verify_win_roundtrip`,
`verify_push_roundtrip`, `verify_loss_roundtrip`, `verify_surrender_roundtrip`

### Scenario harnesses (`blackjack_scenarios.rs`) — ~30–140s each

`scenario_player_natural` (31s), `scenario_dealer_natural` (36s),
`scenario_both_natural` (31s), `scenario_normal_stand` (135s),
`scenario_player_bust` (~84s), `scenario_dealer_bust` (136s),
`scenario_bankroll_conservation` (113s)

### Tic-tac-toe harnesses — ~1–25s each

`position_roundtrip`, `position_to_index_is_always_valid`,
`player_opponent_is_involutive`, `opponent_returns_other_player`,
`square_equality`, `set_marks_occupied`, `get_set_roundtrip`,
`new_board_is_empty`, `no_winner_on_empty_board`,
`winner_detects_row`, `winner_detects_column`, `winner_detects_diagonal`,
`verify_tictactoe_compositional`
