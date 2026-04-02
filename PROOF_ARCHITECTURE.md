# Proof Architecture: Developer Guide

*Hard-won lessons for the next agent working on the verification trifecta.*

This document captures the architectural decisions, toolchain constraints, and
failure modes discovered while building the Kani/Creusot/Verus proof suite for
this codebase.  Read it before touching anything in `crates/strictly_proofs/`.

---

## The Core Principle

This is an **elicitation showcase project**.  Every `#[derive(Elicit)]` type
emits three composable proof methods:

```rust
Type::kani_proof()    -> proc_macro2::TokenStream
Type::creusot_proof() -> proc_macro2::TokenStream
Type::verus_proof()   -> proc_macro2::TokenStream
```

**Your proofs must be built by composing these.**  Do not re-invent invariants
that the elicitation framework already proves.  If you find yourself writing
`#[trusted]`, writing raw arithmetic constraints from scratch, or not calling
`Type::XXX_proof()` anywhere, you have abandoned the library.

The correct pattern is:
```
A::proof() + business logic + B::proof()
```

---

## Toolchain Fundamentals

Each verifier has a fundamentally different invocation model, which drives
every architectural decision below.

| Tool     | Invoked as                              | Workspace resolution | When to compose |
|----------|-----------------------------------------|----------------------|-----------------|
| Kani     | `cargo kani -p strictly_proofs`        | ✅ Full cargo        | At proof-file eval time |
| Creusot  | `opam exec -- cargo creusot`           | ✅ Full cargo        | At proof-file eval time |
| Verus    | `verus --crate-type=lib file.rs`       | ❌ None — standalone | At `cargo build` time (build.rs) |

---

## Kani

### Pattern

Kani runs through cargo, so proof files can freely `use strictly_blackjack::*`.
Harnesses call `Type::kani_proof()` as a building block, then add scenario
logic on top.

```rust
// crates/strictly_proofs/src/kani_proofs/blackjack_scenarios.rs
use strictly_blackjack::{BankrollLedger, Outcome};

#[cfg(kani)]
#[kani::proof]
fn verify_push_identity() {
    let bankroll: u64 = kani::any();
    let bet: u64 = kani::any();
    kani::assume(bet > 0 && bet <= bankroll);

    // Composition: BankrollLedger::kani_proof() gives us the foundation
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).unwrap();
    let (final_balance, _) = ledger.settle(Outcome::Push, token);
    assert!(final_balance == bankroll);
}
```

### Timeout / uncontrolled unwinding

Without a timeout, Kani hangs indefinitely on some harnesses (uncontrolled
loop unrolling).  The tracking recipes use a 300 s timeout:

```bash
just verify-kani-tracked   # 300s default
```

Timed-out harnesses appear as `TIMEOUT` in the CSV (distinct from `FAIL`).

### Loop bounds

Kani derives correct unwind bounds automatically.  Do **not** add
`#[kani::unwind(N)]` — manual unwind limits short-circuit the model checker
and undermine the soundness of the proof.  If a harness hangs, the right fix
is to restructure it (e.g. use fixed-size arrays instead of `Vec` with
`kani::any()` length), not to cap the unroll depth.

---

## Creusot

### Pattern

Creusot is also cargo-based, so proof files can `use strictly_blackjack::*`.
The crucial difference from Kani: Creusot generates **Why3 goals** (VCs) and
discharges them with an ATP solver.  For it to see into a function body, that
function must have `#[ensures]`/`#[requires]` contracts.

**Step 1 — add contracts to the source methods** (in the game crate):

```rust
// crates/strictly_blackjack/src/types.rs
#[cfg_attr(creusot, creusot_contracts::ensures(
    self == Outcome::Push  ==> result@ == bet@,
    self == Outcome::Win   ==> result@ == bet@ * 2,
    // ... etc
))]
pub fn gross_return(&self, bet: u64) -> u64 { /* ... */ }
```

**Step 2 — compose in the proof file:**

```rust
// crates/strictly_proofs/src/creusot_proofs/bankroll_financial.rs
use strictly_blackjack::{BankrollLedger, Outcome};

#[cfg(creusot)]
#[requires(bet > 0u64)]
#[requires(bet <= bankroll)]
#[ensures(result@ == bankroll@)]
pub fn verify_push_identity(bankroll: u64, bet: u64) -> u64 {
    let (ledger, token) = BankrollLedger::debit(bankroll, bet).expect("valid");
    let (final_balance, _) = ledger.settle(Outcome::Push, token);
    final_balance
}
```

Creusot translates the method bodies + contracts to Why3, and the ATP solver
checks that `(bankroll − bet) + bet == bankroll`.

### `#[trusted]` is fatal

`#[trusted]` makes Creusot treat a function as an axiom — **no Why3 VC is
generated**.  A proof file where every function is `#[trusted]` is a proof of
nothing.  The `verify-creusot-prove` recipe will report "0 goals".

**Rule: never use `#[trusted]` in proof files.**  If Creusot cannot see into a
function, add contracts to the source method rather than trusting away the
obligation.

### `opam exec --`

`why3find` and `cargo-creusot` live at `~/.opam/default/bin/`, not on `$PATH`.
Every Creusot recipe in the justfile must use:

```bash
opam exec -- cargo creusot ...
opam exec -- why3find ...
```

The install-check also probes via `opam exec` to avoid false negatives.

### The `@` model operator

In Creusot contracts, `value@` lifts a Rust integer to Why3's unbounded `int`.
Use it in `#[ensures]`/`#[requires]` to avoid overflow in the proof goal:

```rust
#[ensures(result@ == bankroll@ - bet@)]   // ✅ unbounded int arithmetic
#[ensures(result == bankroll - bet)]       // ❌ u64 subtraction may underflow
```

---

## Verus

### The toolchain constraint

Verus is invoked as a **standalone binary**:

```bash
verus --crate-type=lib file.rs
```

It cannot resolve workspace dependencies.  Attempting `use strictly_blackjack::Outcome`
in a Verus proof file fails at parse time.

This is a **toolchain limitation**, not an architectural choice.  Even the
elicitation library's own `elicitation_verus` crate mirrors types locally
rather than importing from `elicitation`.

### Mirror pattern (required)

Each Verus proof file must define the types it reasons about locally:

```rust
verus! {
    // Mirror of strictly_blackjack::Outcome — keep in sync!
    #[derive(PartialEq, Eq)]
    pub enum Outcome { Win, Blackjack, Push, Loss, Surrender }

    // Specification model of BankrollLedger
    pub struct Ledger { pub post_bet_balance: u64, pub bet: u64 }
    // ...
}
```

**Maintenance contract**: when source types change in `strictly_blackjack`,
the mirrors must be updated by hand.  The tracking recipe catches divergence
at verification time.

### Composing `verus_proof()` via build.rs

`Type::verus_proof()` returns a `proc_macro2::TokenStream` — it is callable
from regular Rust, just not from a Verus standalone file.  The solution:
**`build.rs`** acts as the composition site.

```
strictly_blackjack types
        │
        ▼  (called at `cargo build` time)
  build.rs calls Type::verus_proof() → TokenStream
        │  .to_string()
        ▼
  src/verus_proofs/generated/blackjack_foundation.rs
        │
        ▼  (verified by)
  verus --crate-type=lib blackjack_foundation.rs
```

The generated file is self-contained Verus source:

- **Elicitation wrapper stubs** (`U64Default`, etc.) — needed because
  `u64::verus_proof()` generates code that calls `U64Default::new()` /
  `into_inner()`.  The stubs have `ensures` contracts so Verus can verify
  through them.
- **Game type mirrors** — `Outcome` enum as a local definition.
- **Composed proof functions** — output of the `verus_proof()` calls,
  converted to strings.

### Deduplication

`BankrollLedger::verus_proof()` (derive-generated) calls `u64::verus_proof()`
**once per u64 field**.  For a struct with two `u64` fields, this emits
`verify_u64default_identity` twice — a hard Verus error.

The `build.rs` handles this by calling each unique field type's `verus_proof()`
exactly once:

```rust
// build.rs — deduplicated composition
let u64_proof = <u64 as Elicitation>::verus_proof().to_string();  // once
let outcome_proof = Outcome::verus_proof().to_string();           // once
// (NOT BankrollLedger::verus_proof() — that would duplicate u64)
```

This is semantically equivalent to the deduplicated composition of
`BankrollLedger::verus_proof()`.

### `int` vs `u64` in `spec fn`

Verus spec functions promote `u64` arithmetic to unbounded `int`:

```rust
// ❌ Wrong return type
pub open spec fn gross_return(self, bet: u64) -> u64 {
    bet * 2   // type is `int`, not `u64`
}

// ✅ Correct
pub open spec fn gross_return(self, bet: u64) -> int {
    bet as int * 2
}
```

Rules:
- `bet * 2` in a spec context yields `int`, not `u64`
- Struct fields from `u64` need `as u64` when assigned: `(bankroll - bet) as u64`
- Literal `80` in spec context: use `80int` or `80u64` to resolve type inference
- If/else branches must agree on type — `terminal_cols` (`u64`) mixed with
  `label_width + 4` (`int`) requires `terminal_cols as int`

### `.env` loading pitfall

The tracking recipe loads `VERUS_PATH` from `.env`.  The naive pattern breaks:

```bash
# ❌ When VERUS_PATH is absent, `export` with no args dumps the entire env
export $(grep -v '^#' .env | grep VERUS_PATH | xargs)
```

The correct pattern:

```bash
# ✅ Safe grep+sed extraction
RAW_VERUS=$(grep '^VERUS_PATH=' .env | sed 's/^VERUS_PATH=//' | tr -d '"')
VERUS_BIN="${RAW_VERUS/\~/$HOME}"
```

---

## The `verus_proof()` method

`verus_proof()` is part of the `Elicitation` trait and is generated by
`#[derive(Elicit)]`.  Key facts:

- **Signature**: `fn verus_proof() -> proc_macro2::TokenStream` (associated fn)
- **Not feature-gated**: callable from any regular Rust context, including `build.rs`
- **Recursive composition**: for structs, the derived impl calls `verus_proof()` on
  each field type; for multi-variant enums, it calls `verus_multi_variant_enum()`
  which generates a roundtrip identity proof
- **Returns compile-time metadata**: `TokenStream` is not runtime-callable in Verus
  standalone files — that's why `build.rs` materializes it

For struct types, `BankrollLedger::verus_proof()` is literally:

```rust
fn verus_proof() -> TokenStream {
    let mut ts = TokenStream::new();
    ts.extend(<u64 as Elicitation>::verus_proof());  // post_bet_balance
    ts.extend(<u64 as Elicitation>::verus_proof());  // bet
    ts
}
```

For enum types with no field payloads (like `Outcome`), the derive calls
`verus_multi_variant_enum("Outcome")` which emits:

```rust
pub fn verify_outcome_roundtrip(s: Outcome) -> (result: Outcome)
    ensures result == s,
{ s }
```

---

## Proof File Locations

```
crates/strictly_proofs/src/
├── kani_proofs/           # Imported as Rust modules; cargo kani -p strictly_proofs
│   ├── bankroll_financial.rs
│   ├── blackjack_compositional.rs
│   └── ...
├── creusot_proofs/        # Imported as Rust modules; opam exec -- cargo creusot
│   ├── bankroll_financial.rs  ← zero #[trusted]; uses real game type contracts
│   └── ...
└── verus_proofs/          # Verified standalone; NOT imported as Rust modules
    ├── bankroll_financial.rs  ← mirror types + business logic proofs
    ├── tui_breakpoints.rs
    ├── ...
    └── generated/             # AUTO-GENERATED by build.rs
        └── blackjack_foundation.rs  ← composed from verus_proof()
```

The `generated/` directory is produced by `cargo build -p strictly_proofs`.
Both `verify-verus-tracked` and `verify-verus-resume` run this build step
automatically before iterating over proof files.

---

## Justfile Recipes

```bash
just verify-kani-tracked          # Kani; 300s timeout; CSV tracking; [N/TOTAL]
just verify-kani-resume           # skip already-PASS harnesses
just verify-kani-summary          # PASS/FAIL/TIMEOUT breakdown

just verify-creusot-tracked       # Creusot; requires opam + why3find
just verify-creusot-prove         # Run Why3 ATP on all goals

just verify-verus-tracked         # Verus; runs build.rs first; CSV tracking
just verify-verus-resume          # skip already-PASS modules

just verify-all-tracked           # all three tools in sequence
just verify-status                # print CSV summaries from all three
```

---

## Checklist: Adding a New Proof Module

1. **Identify the game type(s)** — which `#[derive(Elicit)]` types are involved?
2. **Add Creusot contracts** to the source methods in the game crate:
   ```rust
   #[cfg_attr(creusot, creusot_contracts::ensures(/* ... */))]
   pub fn my_method(&self) -> u64 { /* ... */ }
   ```
3. **Kani harness**: `use strictly_blackjack::MyType;` and compose
   `MyType::kani_proof()` + scenario logic.
4. **Creusot proof**: `use strictly_blackjack::MyType;` and compose
   `MyType::creusot_proof()` + the new contracts.  Zero `#[trusted]`.
5. **Verus proof**: mirror the types locally; write spec functions with
   `open spec fn`; build business logic proofs.
6. **Verus foundation (build.rs)**: if the new type has fields or is an enum,
   add `MyType::verus_proof()` / `<FieldType as Elicitation>::verus_proof()`
   to `build.rs` and extend the generated file.  Handle deduplication if
   field types overlap with existing types.
7. **Run**: `just verify-all-tracked` — all three must pass.
