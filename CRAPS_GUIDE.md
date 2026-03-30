# Craps as an Elicitation Showcase

> If blackjack shows that a single-bet game
> can be a proof chain, craps shows that a
> *multi-bet, multi-phase, multi-seat* game
> can be one too — with the same zero-cost
> guarantees.

Craps is the second game to be built on the
[Elicitation Framework](https://github.com/crumplecup/elicitation)
in Strictly Games. Where blackjack has a linear
proof chain (bet → play → settle), craps has a
*branching* chain: the come-out roll either
resolves immediately or establishes a point,
forking the control flow into two distinct
type-level paths. The compiler enforces that
you cannot roll during the point phase without
first having established a point, and you
cannot settle bets without having resolved
the round.

---

## The Core Idea

Craps maps onto a **five-phase typestate
machine** where each phase is a distinct Rust
type. Transitions consume `self` and return the
next phase — the compiler refuses to let you
use a phase after it has been consumed.

```text
GameSetup → GameBetting → GameComeOut ──→ GamePointPhase ──→ GameResolved
                              │                                    │
                              └── Natural/Craps ──────────────────►│
```

Each arrow is a method that takes `self`
(by move). Each type carries only the fields
legal for that phase — you cannot access the
established point during betting because the
`GameBetting` type does not have a `point`
field. You cannot mutate bankrolls during
resolution because `GameResolved` only
exposes `&[u64]`.

---

## The Propositions

```rust
/// Proposition: bets have been placed and validated against bankrolls.
#[derive(elicitation::Prop)]
pub struct BetsPlaced;

/// Proposition: a point has been established on the come-out roll.
#[derive(elicitation::Prop)]
pub struct PointEstablished;
```

And from the financial ledger:

```rust
/// Proposition: at least one bet has been deducted from the bankroll.
#[derive(elicitation::Prop)]
pub struct BetDeducted;

/// Proposition: all bets for the round have been correctly settled.
#[derive(elicitation::Prop)]
pub struct RoundSettled;
```

These are zero-byte markers — their entire
purpose is to appear as type parameters
on `Established<P>`, making certain function
signatures uncompilable unless called in
the right order.

---

## The Proof-Carrying Functions

### Step 1: Place Bets

```rust
pub fn execute_place_bets(
    betting: GameBetting,
    seat_bets: Vec<Vec<ActiveBet>>,
) -> Result<(GameComeOut, Established<BetsPlaced>), CrapsError>
```

Validates every seat's total wager against their bankroll. Rejects zero bets.
On success, consumes `GameBetting` and returns `GameComeOut` along with
`Established<BetsPlaced>` — the proof receipt that bets are legally placed.

### Step 2: Come-Out Roll (the fork)

```rust
pub fn execute_comeout_roll(
    comeout: GameComeOut,
    roll: DiceRoll,
    _pre: Established<BetsPlaced>,
) -> ComeOutOutput
```

The `_pre: Established<BetsPlaced>` parameter is the proof requirement.
The compiler will not let you call this function unless you hold a `BetsPlaced`
token. This is the craps equivalent of blackjack's `Established<BetPlaced>`.

`ComeOutOutput` is an enum because two outcomes are possible:

```rust
pub enum ComeOutOutput {
    /// Point was established — carry proof forward.
    PointSet(GamePointPhase, Established<PointEstablished>),
    /// Natural (7/11) or craps (2/3/12) — round resolved immediately.
    Resolved(GameResolved, Established<RoundSettled>),
}
```

This is the key structural difference from
blackjack. Blackjack's fast-finish path
(natural 21) and normal path produce the
*same* final proof (`PayoutSettled`).
In craps, the fork produces *different*
proofs: `PointEstablished` on one branch,
`RoundSettled` on the other. The caller must
pattern-match and handle both paths —
the compiler enforces exhaustive handling.

### Step 3: Point-Phase Rolls (the loop)

```rust
pub fn execute_point_roll(
    point_phase: GamePointPhase,
    roll: DiceRoll,
    _pre: Established<PointEstablished>,
) -> PointRollOutput
```

Requires `Established<PointEstablished>` — you literally cannot call this function
on the natural/craps branch because you do not have the token.

```rust
pub enum PointRollOutput {
    /// Roll did not resolve — carry proof forward for next roll.
    Continue(GamePointPhase, Established<PointEstablished>),
    /// Point made or seven-out — round resolved.
    Resolved(GameResolved, Established<RoundSettled>),
}
```

On `Continue`, the `PointEstablished` proof is recycled — the same pattern as
blackjack's `BetPlaced` recycling during the hit/stand loop. On `Resolved`, the
point phase is consumed and `RoundSettled` is established.

---

## What the Compiler Enforces

You cannot write this:

```rust
// ERROR: missing Established<BetsPlaced>
let output = execute_comeout_roll(comeout, roll, ???);
```

You cannot write this:

```rust
// ERROR: missing Established<PointEstablished>
let output = execute_point_roll(point_phase, roll, ???);
```

You cannot write this:

```rust
// ERROR: wrong proposition type
let bets_proof: Established<BetsPlaced> = /* ... */;
let output = execute_point_roll(point_phase, roll, bets_proof);
//                                                 ^^^^^^^^^^ expected PointEstablished
```

You cannot call `execute_point_roll` after the come-out resolved immediately —
you have no `PointEstablished` to pass in because that state was never reached.
The type system makes the fork visible and prevents misuse.

---

## The Financial Sub-Chain

Craps requires a different financial model than blackjack. In blackjack, exactly
one bet is placed per hand. In craps, a player may have a Pass Line bet, an Odds
bet, a Come bet, and a Place bet *all active simultaneously*.

The `CrapsLedger` handles this:

```text
CrapsLedger::debit(&mut self, amount)  →  Established<BetDeducted>
                                                      │
              (multiple debits per round — &mut self)  │
                                                      ▼
CrapsLedger::settle_round(self, outcomes, Established<BetDeducted>)
                              │
                              ▼
                   (final_bankroll, Established<RoundSettled>)
```

Key differences from blackjack's `BankrollLedger`:

1. **`debit` takes `&mut self`** — not `self`. Multiple bets can be deducted
   incrementally as they are placed during the round.
2. **`settle_round` consumes `self`** — settlement happens exactly once, consuming
   the ledger along with the `BetDeducted` proof. No double-settlement possible.
3. **Outcomes are a slice** — `&[(u64, BetOutcome)]` pairs each wager with its
   resolution (Win/Lose/Push/NoAction).

The proof chain is: you cannot settle without having debited, and you cannot
debit from a consumed ledger. The type system makes the financial lifecycle explicit.

---

## The Progressive Lesson System

Craps is notoriously intimidating. The lesson system gates bet types by house edge,
starting players with the best odds and progressively unlocking riskier bets:

| Level | Bets Unlocked | House Edge | Rounds to Advance |
| ----- | ------------- | ---------- | ----------------- |
| 1 | Pass Line, Don't Pass | 1.36–1.41% | 5 |
| 2 | Pass Odds, Don't Pass Odds | **0%** | 5 |
| 3 | Come, Don't Come, Come/Don't Come Odds | 0–1.41% | 8 |
| 4 | Place (4, 5, 6, 8, 9, 10) | 1.52–6.67% | 8 |
| 5 | Field, Any Seven, Any Craps, Yo, Hi-Lo | 5.56–16.67% | — |

Each level includes educational text explaining what the new bets are, how they
work, and why they have the edge they do. The `LessonProgress` type tracks
the player's level and gates `validate_bet()` — attempting to place a bet
above your lesson level is a type-safe error, not a silent failure.

---

## The 14 Bet Types

Craps supports 14 distinct bet types across 5 categories:

**Line Bets** — The foundation. Pass Line wins on natural (7/11), loses on craps
(2/3/12), and during the point phase wins if the point is made.
Don't Pass is the opposite (bar 12 for push).

**Odds Bets** — The only bet in the casino with **zero house edge**. Pays true
odds behind an existing line bet. Pass Odds pay 2:1 on 4/10, 3:2 on 5/9, 6:5
on 6/8. Don't Pass Odds lay at inverse ratios.

**Come Bets** — Like a Pass Line placed mid-round. The next roll is the come
bet's private "come-out": natural wins, craps loses, otherwise it travels to
that point number.

**Place Bets** — Bet that a specific number (4, 5, 6, 8, 9, 10) hits before 7.
Can be turned on/off at will. Place 6/8 pay 7:6 (1.52% edge), Place 5/9 pay
7:5 (4.00%), Place 4/10 pay 9:5 (6.67%).

**Proposition Bets** — One-roll bets with high house edges. Field (5.56%),
Any Seven (16.67%), Any Craps (11.11%), Yo/Hi-Lo (11.11%).

All payout calculations use **integer ratio arithmetic** — no floating point.
Every payout is `wager * numerator / denominator`, verified by Kani harnesses.

---

## The Typestate Machine in Detail

Each phase is a distinct type with phase-specific fields. Private fields with
getter methods enforce that phases expose only what is legal to read.

**`GameSetup`** — Table configuration. Seats, max odds multiple.

**`GameBetting`** — Bankrolls, shooter index. Players place bets.
Transitions: `start_comeout(seat_bets) → GameComeOut`

**`GameComeOut`** — First roll of the round.
Carries seat bets.
Transitions: `roll(dice) → ComeOutResult
{ PointSet(GamePointPhase)
| Resolved(GameResolved) }`

**`GamePointPhase`** — Point is established
(immutable private field).
Rolls continue. Exposes `seat_bets_mut()` for
adding odds/come bets mid-round.
Transitions: `roll(dice) → PointRollResult
{ Continue(GamePointPhase)
| Resolved(GameResolved) }`

**`GameResolved`** — Round complete. All state frozen for payout calculation.
Transitions: `next_round(updated_bankrolls) → GameBetting`

The cycle `GameBetting → ... → GameResolved → GameBetting` drives multi-round
sessions. Each round runs a fresh proof chain.

---

## Multi-Seat Design

Unlike blackjack's single hand at a time, craps is inherently multi-player.
All seats bet simultaneously, all watch the same dice, but each has independent
bets and bankrolls.

```rust
pub struct CrapsSeat {
    name: String,
    bankroll: u64,
    lesson: LessonProgress,
    active_bets: Vec<ActiveBet>,
    is_shooter: bool,
}
```

The `CrapsTable` manages seats, enforces table min/max limits, validates bets
against lesson level, and rotates the shooter after each round. This structure
supports both human players and AI co-players — each seat's decisions come through
an `ElicitCommunicator`, and the game logic is identical regardless of who sits
in the seat.

---

## Formal Verification: 42 Kani Harnesses

The craps implementation is verified by 42 Kani model-checking harnesses across
three categories:

### Invariants (17 harnesses)

Verify fundamental type properties: dice face bounds, roll sum bounds,
point classification correctness, come-out exhaustiveness/exclusivity
(every roll is *exactly one* of natural, craps, or point), and payout ratio
correctness for all bet types.

### Scenarios (11 harnesses)

Verify game-play correctness: natural 7/11 on come-out resolves pass line win,
craps 2/3/12 resolves pass line loss (with bar-12 push for Don't Pass),
point made resolves pass win, seven-out resolves pass loss, non-resolving rolls
continue the point phase, and individual bet payouts are correct.

### Financial (14 harnesses)

Verify monetary integrity: compositional type proofs (`kani_proof()` legos),
ledger debit arithmetic, overdraft and zero-bet rejection, settlement
correctness for win/loss/push outcomes, specific payout verification for
Place and Pass Line bets, and lesson progression bounds.

Run all craps verification:

```bash
just verify-craps
```

---

## How Craps Differs from Blackjack

- **Proof chain shape** — Blackjack: Linear:
  Bet → Play → Settle. Craps: Branching:
  Bet → ComeOut → (Point loop | Settle).
- **Bets per round** — Blackjack: Exactly 1.
  Craps: Up to 20 per seat.
- **Financial model** — Blackjack:
  `debit(self)` — single bet. Craps:
  `debit(&mut self)` — accumulate bets.
- **Settlement** — Blackjack: Single outcome.
  Craps: Slice of `(wager, BetOutcome)` pairs.
- **Seats** — Blackjack: Single player hand.
  Craps: Multi-seat table.
- **Lesson gating** — Blackjack: None
  (all moves always available). Craps: 5
  levels by house edge.
- **Kani harnesses** — Blackjack: 56.
  Craps: 42.

Despite these differences, the architecture is the same: proof-carrying functions
with `Established<P>` tokens, consuming typestate transitions, and a unified
`ElicitCommunicator` interface for humans and agents alike.

---

## Dice Generation with `#[derive(Rand)]`

Craps is a canonical example of the elicitation
framework's **generator system** — the same
derive-based approach used for prompting and
selection, but applied to random generation.

### The Problem

A naive dice implementation couples game logic
to a specific RNG crate:

```rust
use rand::Rng;

fn roll(rng: &mut impl Rng) -> u8 {
    rng.gen_range(1..=6)
}
```

This makes the game hard to test
deterministically, impossible to replay, and
tightly coupled to a particular `rand` version.

### The Solution: `#[derive(Rand)]`

The elicitation framework provides a `Generator`
trait and a `Rand` derive macro. Adding
`#[derive(Rand)]` to an enum generates a
`random_generator(seed)` method that selects
variants uniformly:

```rust
#[derive(Elicit, Rand)]
pub enum DieFace {
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
}
```

This generates
`DieFace::random_generator(seed: u64)`
returning an `impl Generator<Target = DieFace>`.
Each `.generate()` call produces the next face
in a deterministic sequence.

### Composing Generators: `DiceRoll`

A craps roll needs *two* independent dice.
`DiceRoll` composes two `DieFace` generators
with split seeds to ensure independence:

```rust
impl DiceRoll {
    pub fn random_generator(
        seed: u64,
    ) -> impl Generator<Target = Self> {
        MapGenerator::new(
            RandomGenerator::<u64>::with_seed(seed),
            |inner_seed: u64| {
                let g1 = DieFace::random_generator(
                    inner_seed,
                );
                let g2 = DieFace::random_generator(
                    inner_seed.wrapping_add(1),
                );
                DiceRoll::new(
                    g1.generate(),
                    g2.generate(),
                )
            },
        )
    }
}
```

Each `.generate()` call draws a fresh
`inner_seed` from a `RandomGenerator<u64>`, then
creates two `DieFace` generators with adjacent
seeds — die1 and die2 are never correlated.

### Usage in the TUI

The game loop creates one generator at session
start and passes it through the round:

```rust
let dice = DiceRoll::random_generator(
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .as_nanos() as u64,
);

// Every roll is one call
let roll = dice.generate();
```

### Why This Matters

| Property       | Manual `rand`    | `#[derive(Rand)]`      |
|----------------|------------------|------------------------|
| Deterministic  | If you wire it   | By default (seeded)    |
| Replayable     | Manual work      | Same seed → same game  |
| Testable       | Mock the RNG     | Fixed seed in tests    |
| Version-locked | Yes (`0.8`)      | Framework-managed      |
| Type-safe      | Returns `u8`     | Returns `DieFace`      |

The generator returns `DieFace`, not a raw
integer — the type system guarantees every roll
is in 1..=6, matching the Kani-verified
invariant `dice_face_bounded`. The formal
verification layer and the random generation
layer agree on the same types.

---

## The Elicitation Stack in One Diagram

```text
┌────────────────────────────────────────────────────────────────────┐
│                      run_craps_session                             │
│  (multi-round loop — renders TUI between each elicitation)        │
├────────────────────────────────────────────────────────────────────┤
│                       run_single_round                             │
│                                                                    │
│  execute_place_bets ──→ (GameComeOut, Established<BetsPlaced>)     │
│         │                                                          │
│  execute_comeout_roll ──→ ComeOutOutput                            │
│         │                    /              \                       │
│    PointSet(PointEstablished)    Resolved(RoundSettled)             │
│         │                          (natural/craps — done)          │
│  execute_point_roll (loop)                                         │
│         │                                                          │
│    Continue(PointEstablished)  or  Resolved(RoundSettled)           │
│         │                              │                           │
│         └──── (keep rolling) ─────────►│                           │
│                                        │                           │
│  CrapsLedger::settle_round(outcomes, Established<BetDeducted>)     │
│         │                                                          │
│  (final_bankroll, Established<RoundSettled>)                       │
├────────────────────────────────────────────────────────────────────┤
│                     ElicitCommunicator                              │
│                                                                    │
│  TuiCommunicator            LlmElicitCommunicator                  │
│  (crossterm raw mode)       (API calls to LLM)                     │
│                                                                    │
│  Same elicit() calls. Different prompt rendering.                  │
│  Same proof chain. Different communication channel.                │
└────────────────────────────────────────────────────────────────────┘
```

This is what the elicitation framework enables: a game where the rules are the types,
the moves are the proofs, and the 14-bet complexity of craps is tamed by the same
zero-cost mechanism that handles blackjack's simplicity.
