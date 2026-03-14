# Blackjack for Lawyers — Kani Edition

*A plain-language deep dive into the formal verification proofs that underpin*
*the strictly_games blackjack engine.*

---

## Who this is for

You don't need to write Rust to read this guide. If you've ever asked the
question *"how do I know this software actually does what it claims?"* —
and you want an answer more satisfying than "we tested it a lot" — keep
reading.

---

## The problem with testing

Traditional software testing works by picking specific inputs and checking
the output. A casino's risk team might run a thousand test hands and confirm
the payouts look right. But a thousand tests, no matter how carefully chosen,
can never prove correctness for every possible input. There are billions of
possible combinations of bankroll, bet size, and outcome. A bug can hide
in a corner no tester ever reached.

**Formal verification is different.** Instead of testing samples, it reasons
over *all possible inputs simultaneously* using mathematical logic. Kani —
the tool used here — is a *bounded model checker* developed by Amazon Web
Services. It translates Rust code into mathematical constraints and hands
them to an SMT solver (a theorem prover). The solver either produces a proof
that the property holds for every possible input, or it finds a concrete
counter-example.

When a Kani harness reports PASS, it is not saying "we checked a lot of
cases." It is saying "we proved this property holds for every input in the
stated range, with no exceptions."

---

## What was verified

The proofs are organised into five categories. All 69 harnesses pass.

---

### Category 1 — The type system is sound

**Harnesses:** `verify_rank_compositional`, `verify_suit_compositional`,
`verify_card_compositional`, `verify_outcome_compositional`,
`verify_blackjack_legos`, `verify_bankroll_legos`

Before any game logic runs, Kani verifies that the fundamental data types
are internally consistent.

**Rank** is an enum with 13 variants (Ace through King). The proof confirms
every variant is reachable and well-formed.

**Suit** is an enum with 4 variants (Clubs, Diamonds, Hearts, Spades).
Same treatment.

**Card** is a struct composed of one Rank and one Suit. The compositional
proof confirms that every combination of these two verified types produces
a valid card. There are 52 such combinations; all are proven safe.

**Outcome** is an enum with 5 variants: Loss, Push, Win, Blackjack,
Surrender. Each maps directly to a payout rule. The proof confirms all
five are well-formed.

**BankrollLedger** is the financial record-keeping type. The compositional
proof confirms it composes correctly from two verified `u64` values
(post-bet balance and bet amount).

*What this means in plain language:* The game cannot produce a card that
is simultaneously the Ace of Spades and the Two of Hearts. It cannot invent
a sixth suit. It cannot enter an outcome state that isn't one of the five
defined outcomes. The type system makes these things structurally impossible,
and Kani proves the type system is sound.

---

### Category 2 — The deck and hand logic is correct

**Harnesses:** `deck_has_52_cards`, `deal_reduces_remaining`,
`exhausted_deck_returns_none`, `deck_all_cards_unique`,
`card_value_in_range`, `face_card_values_are_ten`, `ace_raw_value_is_eleven`,
`ace_detection`, `empty_hand_zero_value`,
`hand_value_no_aces`, `hand_value_single_ace_soft`,
`hand_value_ace_busts_soft`, `hand_value_bounds`,
`soft_hard_exact_relation`, `double_ace_value`,
`ace_ace_nine_value`, `ace_ace_ten_soft_collapses`,
`blackjack_requires_two_cards`, `blackjack_biconditional_converse`,
`blackjack_ace_ten`, `blackjack_ace_king`,
`three_card_21_not_blackjack`, `bust_detection`,
`no_bust_under_21`, `exactly_21_not_bust`,
`can_split_matching_ranks`, `cannot_split_different_ranks`,
`cannot_split_wrong_count`, `handvalue_equality`, `card_equality`

#### The deck

**Every new deck has exactly 52 cards.**

```
Property: |Deck::new_shuffled().remaining()| = 52
Result:   PROVED for all shuffles ✓
```

**Dealing a card reduces the count by exactly 1.**

```
Property: deal(deck) ⟹ remaining(deck') = remaining(deck) − 1
Result:   PROVED for all deck states with ≥ 1 card remaining ✓
```

**Dealing from an empty deck returns nothing, not a crash.**

```
Property: remaining(deck) = 0 ⟹ deal(deck) = None
Result:   PROVED — no out-of-bounds access, no panic ✓
```

#### Card values

**Every card has a value between 1 and 11.**

```
Property: ∀c ∈ Card, value(c) ∈ {1, 2, ..., 11}
Result:   PROVED for all 52 possible cards ✓
```

Note: Aces count as 11 at the rank level (adjusted to 1 in hard totals).
Number cards count at face value. Ten, Jack, Queen, King all count as 10.

**Face cards (Ten, Jack, Queen, King) all map to exactly 10 — for every suit.**

```
Property: ∀s ∈ Suit, value(Card(Ten|Jack|Queen|King, s)) = 10
Result:   PROVED parametrically over all four suits ✓
```

This is a key point for counting systems: all four "ten-value" ranks are
provably equivalent regardless of suit.

**The deck has no duplicate cards.**

```
Property: ∀i ≠ j ∈ 0..52, dealt_card(i) ≠ dealt_card(j)
Result:   PROVED — no (Rank × Suit) pair appears more than once ✓
```

The deck is constructed by iterating all 52 (Rank × Suit) combinations
exactly once. Kani proves exhaustively that no two positions hold the
same card — the same Ace of Spades cannot appear twice.

**Ace detection is correct.**

```
Property: is_ace(Card(Ace, _)) = true
Property: is_ace(Card(other, _)) = false for all non-Ace ranks
Result:   PROVED ✓
```

#### Hand values — the soft/hard distinction

Blackjack uses *hard* and *soft* totals. A hard total counts Aces as 1;
a soft total counts one Ace as 11 (if it doesn't cause a bust). The proofs
verify the specific semantics:

**Two non-ace cards sum correctly:**

```
Property: value([2♠, 3♥]) = HandValue { hard: 5, soft: None }
Result:   PROVED ✓
```

**Ace + six creates a soft total:**

```
Property: value([A♠, 6♥]) = HandValue { hard: 7, soft: Some(17) }
Result:   PROVED ✓
```

(Hard 7 counts the Ace as 1; soft 17 counts it as 11.)

**Three-card hands where the Ace must count as 1:**

```
Property: value([A♠, 10♥, 5♦]) = HandValue { hard: 16, soft: None }
Result:   PROVED ✓
```

(An Ace + 10 + 5 with the Ace as 11 would be 26 — bust. So the Ace
must count as 1, giving 16, and there is no soft total.)

**The soft/hard exact relation — only one Ace can be promoted.**

```
Property: ∀h ∈ Hand, soft(h) = Some(s) ⟹ s = hard(h) + 10
Result:   PROVED for all hands up to 7 cards ✓
```

When a soft total exists, it is *exactly* the hard total plus 10 — not
approximately, not sometimes. This proves only one Ace is ever counted
as 11; all others are counted as 1. The mathematics cannot silently
count two Aces as 11 simultaneously, which would give an unearned advantage.

**Ace raw value and hard total:**

```
Property: ∀s ∈ Suit, Card(Ace, s).rank_value() = 11     (card level)
Property: value([A♠]).hard = 1,  value([A♠]).soft = Some(11)
Result:   PROVED ✓
```

**Two-Ace hand:**

```
Property: value([A♠, A♥]) = HandValue { hard: 2, soft: Some(12) }
Result:   PROVED ✓  (one Ace promoted to 11: 1+1+10=12)
```

**Hand value bounds — no overflow:**

```
Property: ∀h ∈ Hand, hard(h) ≤ 127 ∧ (soft(h) = Some(s) ⟹ s ≤ 21)
Result:   PROVED for all hands up to 7 cards ✓
```

Soft totals are only reported when they are ≤ 21 (otherwise there is no
benefit to the soft count). Kani proves this is always the case.

#### Blackjack detection

A blackjack (natural) is specifically a two-card hand totalling 21. A
three-card 21 is not a blackjack and pays at a different rate.

```
Forward:   is_blackjack(h) ⟹ |h| = 2 ∧ value(h) = 21        ✓
Converse:  |h| = 2 ∧ value(h) = 21 ⟹ is_blackjack(h)        ✓
Together:  is_blackjack(h) ⟺ |h| = 2 ∧ value(h) = 21
Result:    PROVED as a true biconditional (both directions) ✓

Property: is_blackjack([A, 10]) = true    ✓
Property: is_blackjack([A, K])  = true    ✓  (King counts as 10)
Property: |h| = 3 ∧ value(h) = 21 ⟹ ¬is_blackjack(h)    ✓
```

Both directions are proven by separate harnesses:
`blackjack_requires_two_cards` (forward) and
`blackjack_biconditional_converse` (converse). No 2-card 21 can
silently fail to be detected as blackjack.

#### Bust detection

```
Property: hard(h) > 21 ⟹ is_bust(h)      ✓
Property: hard(h) ≤ 21 ⟹ ¬is_bust(h)    ✓
Property: hard(h) = 21 ⟹ ¬is_bust(h)    ✓  (exactly 21 is not a bust)
```

#### Split eligibility

Casino rules permit splitting only when the first two cards are of the
same rank. Kani proves the implementation is exact:

```
Property: can_split(h) ⟹ |h| = 2 ∧ rank(h[0]) = rank(h[1])
Property: rank(h[0]) ≠ rank(h[1]) ⟹ ¬can_split(h)
Property: |h| ≠ 2 ⟹ ¬can_split(h)
Result:   All three PROVED ✓
```

---

### Category 3 — The financial settlement is arithmetically exact

This is the most legally significant category. It covers the question:
*does the player receive exactly the right amount of money for each
outcome?*

**Harnesses:** `verify_debit_arithmetic`, `verify_debit_zero_bet_rejected`,
`verify_debit_overdraft_rejected`, `verify_settle_loss`,
`verify_settle_push`, `verify_settle_win`, `verify_settle_blackjack`,
`verify_settle_surrender`, `verify_win_roundtrip`, `verify_push_roundtrip`,
`verify_loss_roundtrip`, `verify_surrender_roundtrip`,
`verify_no_double_deduction`

#### The payout table

| Outcome    | Returns to player           | Net result for player |
|------------|-----------------------------|-----------------------|
| Loss       | 0                           | −bet                  |
| Surrender  | bet / 2                     | −ceil(bet / 2)        |
| Push       | bet (original stake back)   | 0                     |
| Win        | bet × 2                     | +bet                  |
| Blackjack  | bet + floor(bet × 1.5)      | +floor(bet × 1.5)     |

Kani proves every one of these exactly, for all possible `u64` bet values
within the stated assumptions (non-zero bet, sufficient bankroll, no
integer overflow).

#### Bet validation

The engine rejects invalid bets before any money moves:

```
Property: ∀ bankroll, debit(bankroll, 0) = Err(InvalidBet(0))
Result:   PROVED — zero bets always rejected ✓

Property: bet > bankroll ⟹ debit(bankroll, bet) = Err(InsufficientFunds)
Result:   PROVED — overdrafts always rejected ✓
```

#### Debit arithmetic

```
Property: ∀ bankroll, bet | bet > 0 ∧ bet ≤ bankroll ⟹
          debit(bankroll, bet).post_bet_balance = bankroll − bet
Result:   PROVED ✓
```

The post-bet balance is exactly `bankroll − bet`. Kani proves there is
no rounding, no fee, no additional deduction — just the stated bet.

#### Settlement arithmetic (all five outcomes)

**Loss — the player forfeits the bet, receives nothing:**

```
Property: debit(b, x) |> settle(Loss).final = b − x
Result:   PROVED for all u64 b, x where 0 < x ≤ b ✓
```

**Push — original stake returned, net zero:**

```
Property: debit(b, x) |> settle(Push).final = b
Result:   PROVED ✓
```

**Win — player receives original stake plus equal profit:**

```
Property: debit(b, x) |> settle(Win).final = b + x
Result:   PROVED for all valid b, x ✓
```

**Blackjack — 3:2 payout:**

```
Property: settle(Blackjack).final = post_bet_balance + x + floor(x × 1.5)
          Equivalently: gross return = x + (x × 3) / 2
Result:   PROVED ✓
```

Note on integer arithmetic: casino blackjack on odd bets (e.g., $101 at
3:2) conventionally floors the half-chip. Kani proves the implementation
matches this convention exactly:

```
bet = 100 → gross return = 100 + 150 = 250 → net gain = +150
bet = 101 → gross return = 101 + 151 = 252 → net gain = +151  (not 151.5)
```

**Surrender — half stake returned:**

```
Property: debit(b, x) |> settle(Surrender).final = b − ceil(x / 2)
          Equivalently: final = (b − x) + floor(x / 2)
Result:   PROVED ✓
```

On an odd bet the player loses the extra chip: `surrender($101)` returns
`$50`, not `$50.50`. Kani proves this floor behaviour is consistent.

---

### Category 4 — Double-deduction is impossible

**Harness:** `verify_no_double_deduction`

This is the property that distinguishes a *formally verified* financial
system from one that merely relies on correct code paths at runtime.

#### The threat model

A double-deduction bug looks like this: the player bets $100, the engine
deducts $100, the hand is resolved, and the engine then deducts $100
*again* before or during payout. The player loses $200 on a $100 bet.
This was a real defect in earlier versions of this engine.

#### The structural fix — proof-carrying typestate

The fix is not a runtime check. It is a *type system guarantee* enforced
at compile time using the *elicitation framework*:

```
BankrollLedger::debit(bankroll, bet)
    → (BankrollLedger, Established<BetDeducted>)

BankrollLedger::settle(outcome, Established<BetDeducted>)
    → (final_balance, Established<PayoutSettled>)
```

The `Established<BetDeducted>` value is a *proof token* — a value that
can only be created inside `debit` and can only be consumed (used once)
inside `settle`.

Crucially, `Established<BetDeducted>` is **not `Copy`**. In Rust, a
non-Copy value can be used exactly once. After `settle` takes the token,
it is gone. Any attempt to call `settle` a second time with the same token
is a *compile error* — the program will not build:

```
let (ledger, token) = BankrollLedger::debit(bankroll, bet)?;
ledger.settle(Outcome::Win, token);   // token moved here ✓
ledger.settle(Outcome::Win, token);   // ERROR: use of moved value 'token'
                                      //        ↑ the compiler refuses to compile this
```

This means there is no runtime code path that could accidentally settle
twice. The entire class of double-deduction bug is eliminated at compile
time, not guarded against at runtime.

#### What Kani additionally proves

The structural token guarantee rules out double settlement. Kani proves
the arithmetic side — that `settle` itself can only add to the balance:

```
Property: final_balance ≥ post_bet_balance
          (settlement is always additive — never subtracts)
Property: debit(b, x) |> settle(Win).final = b + x
          (the only subtraction path is inside debit, exactly once)
Result:   Both PROVED ✓
```

`settle` uses `Outcome::gross_return` — a function that returns only
non-negative values (the amount to *add back* to the post-bet balance).
There is no subtraction inside `settle`. Kani's symbolic execution
explores every code path and confirms that no path through `settle`
can decrease the balance below `post_bet_balance`.

The combination of the compile-time token linearity guarantee and the
Kani arithmetic proof means:

1. **The bet is deducted exactly once** — structural, compiler enforced.
2. **Settlement only adds to the balance** — arithmetic, Kani proved.
3. **The gross return is correct for every outcome** — arithmetic, Kani proved.

---

### Category 5 — The workflow integration is end-to-end correct

The unit-level proofs verify individual types and functions. The scenario
harnesses verify that the **full call chain** — from bet placement through
card dealing through player actions through dealer resolution through
settlement — produces correct results and always terminates with
`Established<PayoutSettled>`.

These harnesses use `Deck::new_ordered` to construct deterministic decks,
placing exactly the cards needed to produce a specific scenario.

#### Player natural (fast-finish path)

```
Deck:    [Ace♠, Two♥, King♠, Three♥]
         Player gets: Ace + King = 21 (blackjack)
         Dealer gets: Two + Three = 5  (no natural)

Property: execute_place_bet returns PlaceBetOutput::Finished
          (no player actions required)
Property: outcome == Blackjack ∧ bankroll > initial (3:2 payout applied)
Property: Established<PayoutSettled> is present in the return value
Result:   PROVED ✓
```

#### Dealer natural (fast-finish path)

```
Deck:    [Seven♠, Ace♣, Eight♥, King♣]
         Player gets: Seven + Eight = 15 (no natural)
         Dealer gets: Ace + King = 21 (blackjack)

Property: execute_place_bet returns PlaceBetOutput::Finished
Property: outcome == Loss ∧ bankroll == initial − bet
Property: Established<PayoutSettled> is present in the return value
Result:   PROVED ✓
```

#### Both naturals (push, fast-finish)

```
Deck:    [Ace♠, Ace♣, King♠, Queen♣]
         Player gets: Ace + King = 21
         Dealer gets: Ace + Queen = 21

Property: outcome == Push ∧ bankroll == initial (bet fully returned)
Property: Established<PayoutSettled> is present in the return value
Result:   PROVED ✓
```

#### Normal stand path (full chain)

```
Deck:    [King♠, Six♣, King♥, Ten♦, Two♣]
         Player: King+King=20 → Stands
         Dealer: Six+Ten=16 → Hits Two → 18

Property: execute_place_bet returns PlaceBetOutput::PlayerTurn
Property: execute_play_action(Stand) returns Complete(DealerTurn)
Property: execute_dealer_turn returns (GameFinished, PayoutSettled)
Property: outcome == Win ∧ bankroll > initial
Result:   PROVED ✓
```

#### Player bust path

```
Deck:    [Six♠, Two♣, Seven♥, Three♦, Ten♣]
         Player: Six+Seven=13 → Hits Ten → 23 (bust)
         Dealer: Two+Three=5

Property: bust hand transitions through the dealer-turn settlement path
Property: outcome == Loss ∧ bankroll == initial − bet
Property: Established<PayoutSettled> established on bust path ∎
Result:   PROVED ✓
```

#### Dealer bust path

```
Deck:    [Eight♠, Six♣, Nine♥, Seven♦, King♣]
         Player: Eight+Nine=17 → Stands
         Dealer: Six+Seven=13 → Hits King → 23 (bust)

Property: dealer bust yields Win outcome
Property: bankroll > initial after payout
Property: Established<PayoutSettled> established ∎
Result:   PROVED ✓
```

#### Bankroll conservation (symbolic, all valid inputs)

This is the most powerful scenario harness. Rather than checking one
concrete (bankroll, bet) pair, it uses symbolic execution over **all
valid inputs** — every bankroll in [101, 10,000] and every bet in [1, 100]:

```
∀ bankroll ∈ [101, 10_000], bet ∈ [1, 100], bet ≤ bankroll:
  bankroll_after = bankroll_before − bet + gross_return(outcome, bet)

Result:   PROVED ✓
```

This is the integration-layer counterpart to the unit-level `debit_then_settle_win`
round-trip proof. Together they prove financial conservation at every layer
of the stack.

---

## Reading a Kani harness

For the technically curious, here is one complete harness with annotations:

```rust
#[kani::proof]
fn verify_settle_win() {
    // kani::any() means: take ANY u64 value — all 2^64 of them at once.
    let bankroll: u64 = kani::any();
    let bet:      u64 = kani::any();

    // kani::assume() narrows the input space to cases we care about.
    // These are the preconditions of a valid bet:
    kani::assume(bet > 0);                    // bets must be positive
    kani::assume(bet <= bankroll);            // can't bet more than you have
    kani::assume(bet <= u64::MAX / 2);        // prevent overflow in bet * 2
    kani::assume(bankroll <= u64::MAX - bet); // prevent overflow in b + bet

    // Call the actual production code — no mocks, no stubs:
    let (ledger, token) = BankrollLedger::debit(bankroll, bet)
        .expect("valid debit");
    let post_bet = ledger.post_bet_balance();
    let (final_balance, _) = ledger.settle(Outcome::Win, token);

    // assert_eq! is the claim Kani must prove holds for ALL inputs above.
    // If any single input violates this, Kani returns a concrete counter-example.
    assert_eq!(final_balance, post_bet + bet * 2);
    assert_eq!(final_balance, bankroll + bet);
}
```

The `kani::any()` + `kani::assume()` pattern is the core of bounded model
checking. Rather than picking specific values, Kani introduces symbolic
variables representing *all possible values*, then uses the SMT solver to
determine whether the assertions can be violated by any of them. When the
solver says "no violation exists," the harness is proven.

---

## Assumptions and scope

Kani proofs make explicit what they trust — the "cloud of assumptions":

- **Rust's ownership model** — the compiler's borrow checker and move
  semantics are trusted as correct. They are independently verified by
  the Rust project and have no known soundness holes relevant here.
- **u64 arithmetic** — the proofs include overflow bounds (the `assume`
  statements) and are proven within those bounds. Arithmetic outside those
  bounds is not claimed.
- **`Established::assert()`** — this function is the constructor for proof
  tokens. It is a thin wrapper trusted to produce exactly one valid token
  per call. Its implementation is auditable in the elicitation framework.
- **The SMT solver (AWS Z3/CaDiCaL)** — the underlying solver is trusted
  to produce correct proofs. This is an industry-standard tool with its own
  independent formal correctness arguments and decades of production use.

What is *not* in scope:

- **Shuffle fairness** — the randomness of the deck shuffle is a separate
  statistical property not addressed by these proofs.
- **Multi-player accounting** — these proofs cover one player's bankroll
  in isolation.
- **Network or persistence layer** — the proofs cover pure in-memory
  game logic; transport and storage correctness are separate concerns.
- **Dealer strategy** — whether the dealer plays correctly by house rules
  is a game AI question, not a financial soundness question.

---

## Summary of proof counts

| Category                          | Harnesses | Status      |
|-----------------------------------|-----------|-------------|
| Type system / compositional       | 5         | ✅ All pass |
| Deck and hand logic               | 30        | ✅ All pass |
| Financial settlement              | 14        | ✅ All pass |
| Tic-tac-toe (bonus)               | 13        | ✅ All pass |
| Workflow integration (scenarios)  | 7         | ✅ All pass |
| **Total**                         | **69**    | **69/69**   |

---

## What "69/69 pass" means for confidence

A regulator reviewing a traditional gambling system asks for test logs,
code reviews, and statistical audits. These are evidence of quality but
not proofs of correctness. A sufficiently clever bug can evade all of them.

Kani's 69/69 passing harnesses are a different class of evidence. For the
properties stated — payout arithmetic, bet validation, double-deduction
impossibility, hand value semantics, deck integrity, and end-to-end workflow
correctness — the proofs are **exhaustive over the input domain**. There is
no input within the stated preconditions for which any of these properties fail.

This does not mean the software is perfect in every dimension. Shuffle
fairness, UI correctness, and network reliability are outside the proof
scope. But for the financial core — the question of whether a player
receives exactly the right amount of money for each outcome — the answer
is not "we think so" or "testing shows it." The answer is:

**It is mathematically proven.**

---

*Proofs are in `crates/strictly_proofs/src/kani_proofs/`.*
*Run them with:*

```bash
just verify-kani-tracked          # Run all 69 harnesses; write per-harness CSV
just verify-kani-resume           # Resume after interruption (skips already-PASS)
just verify-kani-summary          # Print pass/fail totals from last run
just verify-kani-failed           # List only the failing harnesses
cargo kani -p strictly_proofs     # One-shot (no per-harness tracking)
```
