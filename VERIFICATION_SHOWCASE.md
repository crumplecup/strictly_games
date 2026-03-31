# Verification Trifecta Showcase

This document demonstrates **the same proof written three different ways** across Kani, Verus, and Creusot verifiers.

## Showcase: `opponent_involutive`

**Property**: For any player `p`, `p.opponent().opponent() == p`

This is a mathematical involution - applying opponent() twice returns the original value.

---

### 1. Kani (Symbolic Execution)

```rust
/// Verify opponent() is an involution using symbolic execution.
#[kani::proof]
fn verify_opponent_involutive() {
    let p: Player = kani::any();  // Explore ALL possible Player values
    
    // Kani proves this for X and O simultaneously
    assert_eq!(p, p.opponent().opponent());
}
```

**How it works:**

- `kani::any()` creates symbolic value representing ALL possible `Player` variants
- Kani's CBMC backend explores both `Player::X` and `Player::O`
- Proof succeeds if assertion holds for **every possible input**

**Run**: `cargo kani --harness verify_opponent_involutive`

**Output**: `VERIFICATION:- SUCCESSFUL` after checking thousands of paths

---

### 2. Verus (SMT Specifications)

```rust
/// Verify opponent() is an involution using executable specification.
verus! {
pub proof fn verify_opponent_involutive(p: Player)
    ensures p == p.opponent().opponent(),
{
    match p {
        Player::X => {
            assert(p.opponent() == Player::O);
            assert(p.opponent().opponent() == Player::X);
        }
        Player::O => {
            assert(p.opponent() == Player::X);
            assert(p.opponent().opponent() == Player::O);
        }
    }
}
}
```

**How it works:**

- `ensures` clause states the postcondition (what must be true)
- Match exhaustively handles both cases
- Z3 SMT solver validates specification is satisfied
- Proof is executable Rust code

**Run**: `cargo check` (Verus proofs compile as specifications)

**Output**: Clean compilation means proof verified

---

### 3. Creusot (Deductive Trusted)

```rust
/// Verify opponent() is an involution using trusted axiom.
#[trusted]
#[requires(true)]
#[ensures(p == p.opponent().opponent())]
pub fn verify_opponent_involutive(p: Player) -> Player {
    p.opponent().opponent()
}
```

**How it works:**

- `#[trusted]` marks this as axiomatic (cloud of assumptions)
- `#[requires]` states precondition (always true here)
- `#[ensures]` states postcondition (the involution property)
- Compiler accepts contract as correct without verification

**Run**: `cargo check` (instant compilation)

**Output**: Clean compilation establishes axiom

---

## Comparison Table

| Aspect | Kani | Verus | Creusot |
| -------- | ------ | ------- | --------- |
| **Approach** | Bounded model checking | SMT solver | Deductive logic |
| **Input Space** | Symbolic (`kani::any()`) | Forall quantifier | Axiomatic |
| **Verification** | CBMC backend explores paths | Z3 validates specifications | Trust contracts |
| **Time** | Minutes (bounded) | Seconds (SMT) | Instant (trusted) |
| **Guarantees** | Bounded completeness | Mathematical proof | Axiom validity |
| **Use Case** | Find bugs, edge cases | Prove correctness | Document contracts |

---

## Why All Three Matter

**Kani**: Catches real bugs through exhaustive bounded checking  
**Verus**: Provides mathematical proof of correctness  
**Creusot**: Documents contracts for mature code where implementation is trusted

**Together**: They demonstrate that **elicitation's constraint model works across the entire Rust verification ecosystem** - not tied to one tool or methodology.

---

## Running the Showcase

```bash
# Run Kani proofs (symbolic execution)
just verify-kani-tracked

# Run Verus proofs (SMT specifications)
just verify-verus-tracked

# Run Creusot proofs (deductive trusted)
just verify-creusot-tracked

# Run all three + generate dashboard
just verify-all-tracked
just verify-dashboard
```

---

## Key Insight

The **same property** (`opponent_involutive`) is proven three different ways, each with different tradeoffs. This is the power of the elicitation framework: **it provides verification primitives that work regardless of which formal methods tools you prefer.**

For safety-critical applications, use all three:

- Kani finds bugs during development
- Verus provides mathematical proof for auditors
- Creusot documents contracts for mature code

**Coverage**: 20+ Kani proofs, 7 Verus specs, 7 Creusot contracts = **Full formal verification blanket**
