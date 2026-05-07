//! `strictly_verus` — self-contained Verus proof targets for strictly_games VSMs.
//!
//! The actual verification content lives in `src/generated/` — files produced by
//! `build.rs` that define inline stub types and `assume_specification` contracts.
//! Those files are run directly through the Verus binary; this lib crate is a
//! placeholder that allows `cargo build -p strictly_verus` to regenerate them.
//!
//! # Regenerating
//!
//! ```bash
//! cargo build -p strictly_verus
//! ```
//!
//! # Verifying
//!
//! ```bash
//! just verify-verus-vsm   # from strictly_games root
//! ```
