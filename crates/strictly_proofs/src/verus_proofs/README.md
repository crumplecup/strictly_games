# Verus Formal Verification

This directory contains Verus proofs using executable specifications with `ensures` clauses.

## Verus vs Kani

**Kani** (symbolic execution):

- Uses `kani::any()` to explore all possible inputs
- Bounded model checking with CBMC
- Great for finding counterexamples

**Verus** (specification-based):

- Uses `ensures` clauses to specify correctness
- Proves with Z3 SMT solver
- More expressive for complex properties

Both provide compositional verification through Elicitation framework.

## Pattern from Elicitation

```rust
verus! {
    /// Verify contract correctness with ensures clause
    pub proof fn verify_opponent_involutive(p: Player)
        ensures p == p.opponent().opponent(),
    {
        // Z3 proves via exhaustive enum matching
        match p {
            Player::X => assert(p.opponent().opponent() == Player::X),
            Player::O => assert(p.opponent().opponent() == Player::O),
        }
    }
}
```

## Proof Structure

### 1. `compositional_proof.rs`

Witnesses that types inherit verification through `#[derive(Elicit)]`.

### 2. `game_invariants.rs`

Proves game rules:

- `verify_opponent_involutive`: opponent(opponent(p)) = p
- `verify_position_to_index_valid`: Indices always in bounds
- `verify_new_board_empty`: New board is empty everywhere

### 3. `passive_affirm.rs`

Proves escape hatch pattern:

- `verify_affirm_continue_returns_bool`: Always terminates
- `verify_new_session_not_cancelled`: Correct initialization
- `verify_multiple_cancels_idempotent`: Idempotency

### 4. `tui_breakpoints.rs`

Proves NoOverflow TUI layout arithmetic using Z3 SMT:

- `truncation_output_bounded`: `truncated_width(w, m) ≤ m` (universal, no assumptions)
- `truncation_identity`: `w ≤ m → truncated_width(w, m) = w`
- `truncation_always_satisfies_label_contained`: truncation satisfies LabelContained for ALL inputs — the Z3 counterpart to Kani's bounded-model-check proof
- `node_box_width_no_overflow`: `(label+4).min(cols) ≤ u16::MAX` for terminals ≤ 200 cols
- `area_zero_height_triggers_failure` / `area_nonzero_height_does_not_fail`: AreaSufficient prop
- `breakpoint_minimum_layout`: 80×24 column + row arithmetic
- `breakpoint_ultrawide_layout`: 200×60 column + row arithmetic
- `breakpoint_micro_expected_failure`: 40×12 provably too small
- `breakpoint_tiny_graceful_degrade`: 60×20 exhausts row budget
- `symbolic_must_pass_range`: ∀ (cols,rows) ∈ [80..200]×[24..60]: both invariants hold

## Running Verus

Install Verus:

```bash
git clone https://github.com/verus-lang/verus
cd verus/source
./tools/get-z3.sh
cargo build --release
```

Verify strictly_games:

```bash
~/repos/verus/source/target-verus/release/verus \
    --crate-type=lib \
    src/lib.rs
```

## Cloud of Assumptions

**Trust:**

- Rust's type system (exhaustive matching, bounds checking)
- Standard library (Vec, tokio::watch)
- Z3 SMT solver correctness

**Verify:**

- Our game logic (opponent, winner detection)
- Our wrapper code (Board::get/set)
- Control flow (Passive-Affirm cancellation)

Same philosophy as Kani proofs, different verification engine.
