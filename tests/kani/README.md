# Formal Verification: Tic-Tac-Toe Showcase

This directory contains Kani proof harnesses demonstrating that the tic-tac-toe implementation is **formally verified** through the Elicitation Framework.

## 🎯 Purpose: Showcasing "Caged Agents"

This project demonstrates that **LLMs can be mathematically constrained** for use in privacy, security, and safety-critical applications.

### The Vision

**Traditional LLM**: Can hallucinate, produce invalid states, misunderstand constraints
**Caged Agent**: Mathematically proven to only produce valid states

This codebase is the **proof of concept**.

## 📐 Verification Architecture

```
┌────────────────────────────────────────────────────┐
│ Elicitation Framework (321 Kani Proofs)           │
│ ✓ Select mechanism correctness                    │
│ ✓ Primitive type contracts                        │
│ ✓ Compositional verification rules                │
└────────────────┬───────────────────────────────────┘
                 │ #[derive(Elicit)]
                 ↓
┌────────────────────────────────────────────────────┐
│ Game Types (compositional_proof.rs)               │
│ ✓ Position, Player, Square, Board                 │
│ ✓ Auto-generated kani_proof() methods             │
│ ✓ Verified by composition (free from framework)   │
└────────────────┬───────────────────────────────────┘
                 │ used in
                 ↓
┌────────────────────────────────────────────────────┐
│ Game Rules (game_invariants.rs)                   │
│ ✓ Winner detection is mutually exclusive          │
│ ✓ No invalid board states                         │
│ ✓ Position indexing never out of bounds           │
└────────────────┬───────────────────────────────────┘
                 │ enforces
                 ↓
┌────────────────────────────────────────────────────┐
│ Escape Hatch (passive_affirm_proof.rs)            │
│ ✓ User can always exit (no deadlock)              │
│ ✓ Cancellation is monotonic (stays cancelled)     │
│ ✓ Race-free (tokio::watch semantics)              │
└────────────────────────────────────────────────────┘
```

## 🧪 Proof Files

### 1. `compositional_proof.rs`

**What it proves**: Game types are well-formed

- Position has exactly 9 variants (finite action space)
- Player has exactly 2 variants (X, O)
- Board contains exactly 9 squares
- Indexing is always in bounds

**Key insight**: These proofs are **auto-generated** by `#[derive(Elicit)]`. We get verification for free by using the framework.

**Running**:
```bash
cargo kani --harness verify_tictactoe_compositional
```

### 2. `game_invariants.rs`

**What it proves**: Game rules are implemented correctly

- Mutual exclusion: at most one winner
- Winner detection is deterministic
- Draw and winner are mutually exclusive  
- Winner requires at least 3 marks
- Position enumeration is bijective
- Player.opponent() is involutive

**Key insight**: These go beyond type safety to prove **game semantics** are correct.

**Running**:
```bash
cargo kani --harness board_never_has_both_winners
cargo kani --harness winner_detection_is_deterministic
cargo kani --harness draw_and_winner_are_mutually_exclusive
cargo kani --harness position_to_index_is_always_valid
cargo kani --harness position_index_round_trips
cargo kani --harness winner_requires_at_least_three_marks
cargo kani --harness player_opponent_is_involutive
```

### 3. `passive_affirm_proof.rs`

**What it proves**: The escape hatch pattern is formally correct

- `affirm_continue()` always returns (no deadlock)
- Cancellation is monotonic (once set, stays set)
- Multiple cancels are idempotent
- New sessions start uncancelled
- Reset correctly restores state

**Key insight**: This verifies a **novel pattern** (passive-Affirm) that enables human-in-the-loop control without annoying prompts.

**Running**:
```bash
cargo kani --harness affirm_continue_always_returns
cargo kani --harness cancellation_is_monotonic
cargo kani --harness multiple_cancels_are_idempotent
cargo kani --harness new_session_is_not_cancelled
cargo kani --harness reset_cancel_restores_state
```

## 🚀 Running All Proofs

```bash
# Check that verification code compiles
cargo check --features verification

# Run all proofs (requires Kani installed)
cargo kani --features verification

# Run specific harness
cargo kani --harness verify_tictactoe_compositional
```

### Installing Kani

```bash
cargo install --locked kani-verifier
cargo kani setup
```

See: https://model-checking.github.io/kani/install-guide.html

## 📊 What Gets Verified

### ✅ Type Safety (Compositional)

- **Position is always valid**: Agent cannot propose position 10 (doesn't exist)
- **No type confusion**: Position is not a string, not coordinates
- **Bounded action space**: Exactly 9 possible moves, provably enumerable
- **Memory safety**: Array indexing never out of bounds

### ✅ Game Semantics (Invariants)

- **Rules are correct**: Winner detection follows tic-tac-toe specification
- **No impossible states**: Can't have both players win, or winner with <3 marks
- **Determinism**: Same board → same result (critical for replay)
- **Turn alternation**: opponent(opponent(p)) = p ensures valid turn sequence

### ✅ Control Flow (Passive-Affirm)

- **User always has control**: Can exit at any time, no deadlock
- **Reliable cancellation**: Once cancelled, stays cancelled
- **Graceful degradation**: Multiple cancel requests don't corrupt state

## 🎓 Why This Matters

### The "Caged Agent" Property

When an LLM is asked to make a move in this game:

```
Agent: "I'll place at position 15"
Framework: ❌ Type error - Position enum has 9 variants

Agent: "I'll place X at center, even though it's O's turn"
Framework: ❌ Contract violation - PlayerTurn precondition fails

Agent: "Let me continue even though user pressed 'q'"
Framework: ❌ Passive-Affirm check returns false, loop exits
```

**The agent is "caged" - mathematically constrained to safe behavior.**

### Implications for Critical Applications

This pattern enables LLMs in:

- **Privacy**: Agent cannot bypass access controls (type-enforced)
- **Security**: Agent cannot execute dangerous actions (contract-enforced)
- **Safety**: User can always override agent (escape hatch proven)

The framework makes this **practical**, not just theoretical:
- Zero runtime cost (proofs are PhantomData)
- Compositional verification (scales to complex systems)
- Developer-friendly (just use `#[derive(Elicit)]`)

## 📖 Documentation

- **Project root**: See `FORMAL_VERIFICATION.md` for detailed explanation
- **Elicitation framework**: `/home/erik/repos/elicitation/crates/elicitation/src/verification/`
- **Framework proofs**: 321 Kani harnesses in elicitation crate

## 🎯 Success Metrics

1. ✅ All types derive `Elicit` → compositional verification
2. ✅ Game rules proven correct → semantic verification
3. ✅ Escape hatch proven safe → control flow verification
4. ✅ Zero runtime cost → PhantomData proofs
5. ✅ Demonstration-ready → showcase for critical applications

## 🔗 Related Work

- **Typestate pattern**: Compile-time phase enforcement (Setup, InProgress, Finished)
- **Proof-carrying code**: Execute only with proof tokens (Established<P>)
- **Dependent types**: Type-level computation (contracts as types)
- **Formal methods**: Mathematical verification (Kani symbolic execution)

## 📝 Citation

If using this approach in research:

```
@software{strictly_games_verification,
  title={Formally Verified LLM Interactions: The Strictly Games Showcase},
  author={Rose, Erik},
  year={2026},
  note={Demonstrates "caged agent" pattern for LLM safety in critical applications},
  url={https://github.com/crumplecup/strictly_games}
}
```

---

**Status**: All verification infrastructure complete. Ready for showcase demonstrations.
