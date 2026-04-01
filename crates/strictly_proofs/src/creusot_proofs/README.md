# Creusot Formal Verification

Creusot proofs using `#[trusted]` annotations with "cloud of assumptions" pattern.

## Pattern from Elicitation (456 Proofs)

```rust
#[trusted]  // Trust implementation
#[requires(true)]  // Precondition
#[ensures(p == p.opponent().opponent())]  // Postcondition
pub fn verify_opponent_involutive(p: Player) -> Player {
    p.opponent().opponent()
}
```

## Cloud of Assumptions

**Trust:** Rust stdlib, tokio, type system
**Verify:** Game logic, wrapper code, contracts

## Proof Count

- Compositional: 2 proofs
- Game invariants: 3 proofs
- Passive-Affirm: 2 proofs
- TUI breakpoints: 9 proofs (truncation, overflow, area, breakpoint witnesses)

### Total: 16 Creusot proofs

### 4. `tui_breakpoints.rs`

Proves NoOverflow TUI layout arithmetic:

- `creusot_truncation_output_bounded`: `truncated_width(w, m) ≤ m` (universal)
- `creusot_truncation_identity`: `w ≤ m → truncated_width(w, m) = w`
- `creusot_truncation_satisfies_label_contained`: truncation output fits in bordered cell (universal)
- `creusot_node_box_no_overflow`: `(label+4).min(cols) ≤ u16::MAX` for terminals ≤ 200 cols
- `creusot_area_zero_height_fails`: zero-height area with content triggers failure
- `creusot_area_sufficient_passes`: non-zero height with fitting content passes
- `creusot_breakpoint_minimum_fits`: 80×24 fits 4 nodes + prompt (witness)
- `creusot_breakpoint_micro_expected_failure`: 40×12 provably too small (witness)
- `creusot_breakpoint_tiny_graceful_degrade`: 60×20 exhausts row budget (witness)

```bash
cargo install cargo-creusot
cargo creusot verify src/creusot_proofs/
```

Pattern: elicitation's 456 trusted proofs with instant compilation.
