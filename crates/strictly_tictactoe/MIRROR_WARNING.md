# ⚠️ VERUS MIRROR WARNING

## Types in this crate are mirrored for formal verification

The following types have **duplicate definitions** in:
```
strictly_proofs/src/verus_proofs/game_invariants.rs
```

### Mirrored Types:
- `Player` enum (`types.rs`)
- `Position` enum (`position.rs`)
- `Square` enum (`types.rs`)
- `Board` struct (`types.rs`)

## MUST SYNC MANUALLY

When you change these types, you **MUST** update the mirror:

1. Open `strictly_proofs/src/verus_proofs/game_invariants.rs`
2. Find the mirrored type definition
3. Update enum variants, method signatures, invariants
4. Run `just verify-verus-tracked` to validate

### No Automatic Detection

There is **no compiler warning** when the mirror diverges. The types will silently drift out of sync until Verus proofs fail or prove wrong properties.

This is technical debt. See `strictly_proofs/src/verus_proofs/README.md` for details.

## Why This Pattern?

Verus cannot resolve workspace dependencies. This mirrors elicitation's proven approach (`crates/elicitation_verus/`).

We're waiting for cargo-verus integration or native workspace support.
