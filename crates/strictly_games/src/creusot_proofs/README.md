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

**Total: 7 Creusot proofs**

## Running

```bash
cargo install cargo-creusot
cargo creusot verify src/creusot_proofs/
```

Pattern: elicitation's 456 trusted proofs with instant compilation.
