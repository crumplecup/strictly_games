# Justfile for strictly_games
# See: https://github.com/casey/just

# List all available recipes
default:
    @just --list

# Run all tests
test:
    cargo test

# Run API tests (requires valid API keys, uses tokens)
test-api:
    cargo test --features api

# Run clippy linter
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Run all checks (clippy + fmt + test)
check-all: clippy fmt-check test

# Build the project
build:
    cargo build

# Build release version
build-release:
    cargo build --release

# Run the server
run:
    cargo run

# Clean build artifacts
clean:
    cargo clean

# ============================================================================
# Formal Verification (Kani)
# ============================================================================

# Run all Kani verification harnesses
verify:
    @echo "Running Kani formal verification..."
    cargo kani --features verification

# Run compositional proof (types verified through framework)
verify-compositional:
    @echo "Verifying types through compositional proof chain..."
    cargo kani --harness verify_tictactoe_compositional

# Run game invariant proofs (game rules correctness)
verify-invariants:
    @echo "Verifying game-specific invariants..."
    cargo kani --harness board_never_has_both_winners
    cargo kani --harness winner_detection_is_deterministic
    cargo kani --harness draw_and_winner_are_mutually_exclusive
    cargo kani --harness position_to_index_is_always_valid
    cargo kani --harness position_index_round_trips
    cargo kani --harness winner_requires_at_least_three_marks
    cargo kani --harness player_opponent_is_involutive

# Run passive-affirm escape hatch proofs
verify-passive-affirm:
    @echo "Verifying passive-Affirm escape hatch pattern..."
    cargo kani --harness affirm_continue_always_returns
    cargo kani --harness cancellation_is_monotonic
    cargo kani --harness multiple_cancels_are_idempotent
    cargo kani --harness new_session_is_not_cancelled
    cargo kani --harness reset_cancel_restores_state

# Check that verification code compiles (fast check before running Kani)
verify-check:
    @echo "Checking verification code compiles..."
    cargo check --features verification

# Install Kani verifier (one-time setup)
install-kani:
    @echo "Installing Kani Rust Verifier..."
    cargo install --locked kani-verifier
    cargo kani setup
    @echo "Kani installed. Run 'just verify' to run proofs."

# Show Kani version
kani-version:
    cargo kani --version
