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

# Run the MCP server (stdio mode)
run:
    cargo run -- server

# Run the HTTP server
run-http:
    cargo run -- http

# Run the TUI client
run-tui:
    cargo run -- tui

# Run the lobby
run-lobby:
    cargo run -- lobby

# Clean build artifacts
clean:
    cargo clean

# ============================================================================
# Formal Verification (Kani)
# ============================================================================

# Run all Kani verification harnesses
verify:
    @echo "Running Kani formal verification..."
    cargo kani

# Run compositional proof (types verified through framework)
verify-compositional:
    @echo "Verifying types through compositional proof chain..."
    cargo kani --harness verify_tictactoe_compositional

# Run game invariant proofs (game rules correctness)
verify-invariants:
    @echo "Verifying game-specific invariants..."
    cargo kani --harness player_opponent_is_involutive
    cargo kani --harness position_to_index_is_always_valid

# Run passive-affirm escape hatch proofs
verify-passive-affirm:
    @echo "Verifying passive-Affirm escape hatch pattern..."
    cargo kani --harness affirm_continue_always_returns
    cargo kani --harness cancellation_is_monotonic

# Check that verification code compiles (fast check before running Kani)
verify-check:
    @echo "Checking verification code compiles..."
    cargo check

# Tracked verification (CSV output with timestamps)
verify-kani-tracked:
    @echo "Running tracked Kani verification..."
    cargo run --bin strictly_games -- verify --tool kani

# Run Verus verification with CSV tracking
verify-verus-tracked:
    @echo "Running tracked Verus verification..."
    cargo run --bin strictly_games -- verify --tool verus

# Run Creusot verification with CSV tracking
verify-creusot-tracked:
    @echo "Running tracked Creusot verification..."
    cargo run --bin strictly_games -- verify --tool creusot

# Show current verification status from CSV
verify-status:
    @echo "Verification status:"
    @echo "Run 'just verify-all-tracked' to see all results"

# Run all tracked verification (Kani + Verus + Creusot)
verify-all-tracked:
    @echo "Running verification trifecta..."
    cargo run --bin strictly_games -- verify --tool all

# Generate verification dashboard from CSV
verify-dashboard:
    @echo "Generating verification dashboard..."
    python3 scripts/verification_dashboard.py

# Install Kani verifier (one-time setup)
install-kani:
    @echo "Installing Kani Rust Verifier..."
    cargo install --locked kani-verifier
    cargo kani setup
    @echo "Kani installed. Run 'just verify' to run proofs."

# Show Kani version
kani-version:
    cargo kani --version
