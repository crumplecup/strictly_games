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

# Run the default command (TUI lobby)
run:
    cargo run -- tui

# Run the HTTP server
run-http:
    cargo run -- http

# Run the TUI lobby
run-tui:
    cargo run -- tui

# Clean build artifacts
clean:
    cargo clean

# ============================================================================
# Formal Verification (Kani)
# ============================================================================

# Run all Kani verification harnesses
verify:
    @echo "Running Kani formal verification..."
    cargo kani -p strictly_proofs

# Run compositional proof (types verified through framework)
verify-compositional:
    @echo "Verifying types through compositional proof chain..."
    cargo kani -p strictly_proofs --harness verify_tictactoe_compositional
    cargo kani -p strictly_proofs --harness verify_blackjack_legos
    cargo kani -p strictly_proofs --harness verify_bankroll_legos
    cargo kani -p strictly_proofs --harness verify_craps_legos

# Run game invariant proofs (game rules correctness)
verify-invariants:
    @echo "Verifying game-specific invariants..."
    cargo kani -p strictly_proofs --harness player_opponent_is_involutive
    cargo kani -p strictly_proofs --harness position_to_index_is_always_valid

# Run financial typestate proofs (BankrollLedger double-deduction safety)
verify-financial:
    @echo "Verifying financial typestate proofs (BankrollLedger)..."
    cargo kani -p strictly_proofs --harness verify_bankroll_legos \
        --harness verify_debit_arithmetic \
        --harness verify_debit_zero_bet_rejected \
        --harness verify_debit_overdraft_rejected \
        --harness verify_settle_loss \
        --harness verify_settle_push \
        --harness verify_settle_win \
        --harness verify_settle_blackjack \
        --harness verify_settle_surrender \
        --harness verify_no_double_deduction \
        --harness verify_win_roundtrip \
        --harness verify_push_roundtrip \
        --harness verify_loss_roundtrip \
        --harness verify_surrender_roundtrip

# Run craps invariant, scenario, and financial proofs
verify-craps:
    @echo "Verifying craps game logic..."
    cargo kani -p strictly_proofs \
        --harness die_face_value_bounded \
        --harness dice_roll_sum_bounded \
        --harness die_face_roundtrip \
        --harness point_values_are_valid \
        --harness point_roundtrip \
        --harness seven_is_not_a_point \
        --harness craps_numbers_are_not_points \
        --harness comeout_classification_exhaustive \
        --harness comeout_classification_exclusive \
        --harness natural_values_correct \
        --harness craps_values_correct \
        --harness pass_line_payout_is_even_money \
        --harness dont_pass_payout_is_even_money \
        --harness place_six_eight_payout \
        --harness place_five_nine_payout \
        --harness place_four_ten_payout \
        --harness house_edge_non_negative \
        --harness comeout_natural_seven \
        --harness comeout_natural_eleven \
        --harness comeout_craps_two \
        --harness comeout_craps_three \
        --harness comeout_craps_twelve \
        --harness point_made_eight \
        --harness seven_out \
        --harness point_phase_no_decision \
        --harness pass_line_wins_on_natural \
        --harness dont_pass_wins_on_craps_two \
        --harness dont_pass_pushes_on_twelve \
        --harness verify_craps_legos \
        --harness debit_single_bet_correct \
        --harness debit_rejects_over_bankroll \
        --harness debit_rejects_zero_bet \
        --harness settle_win_returns_correct_balance \
        --harness settle_loss_reduces_balance \
        --harness settle_push_returns_wager \
        --harness pass_line_win_payout_correct \
        --harness place_six_win_payout_correct \
        --harness place_five_win_payout_correct \
        --harness win_payout_never_zero \
        --harness lesson_level_bounded \
        --harness at_level_clamps \
        --harness lesson_advancement_monotonic

# Run passive-affirm escape hatch proofs
verify-passive-affirm:
    @echo "Verifying passive-Affirm escape hatch pattern..."
    cargo kani -p strictly_proofs --harness affirm_continue_always_returns
    cargo kani -p strictly_proofs --harness cancellation_is_monotonic

# Check that verification code compiles (fast check before running Kani)
verify-check:
    @echo "Checking verification code compiles..."
    cargo check -p strictly_proofs

# Run each Kani harness individually, recording pass/fail/duration to a CSV.
# Format: module,harness,status,duration_secs,timestamp
# Usage: just verify-kani-tracked            (fresh run, overwrites CSV)
#        just verify-kani-tracked my.csv     (custom CSV path)
verify-kani-tracked csv="kani_verification_results.csv":
    #!/usr/bin/env bash
    set -euo pipefail
    CSV="{{csv}}"
    echo "module,harness,status,duration_secs,timestamp" > "$CSV"
    PASS=0; FAIL=0
    HARNESSES=(
        ace_ace_nine_value
        ace_ace_ten_soft_collapses
        ace_detection
        ace_raw_value_is_eleven
        blackjack_ace_king
        blackjack_ace_ten
        blackjack_biconditional_converse
        blackjack_requires_two_cards
        bust_detection
        cannot_split_different_ranks
        cannot_split_wrong_count
        can_split_matching_ranks
        card_equality
        card_value_in_range
        deal_reduces_remaining
        deck_all_cards_unique
        deck_has_52_cards
        double_ace_value
        empty_hand_zero_value
        exactly_21_not_bust
        exhausted_deck_returns_none
        face_card_values_are_ten
        get_set_roundtrip
        hand_value_ace_busts_soft
        hand_value_bounds
        handvalue_equality
        hand_value_no_aces
        hand_value_single_ace_soft
        new_board_is_empty
        no_bust_under_21
        no_winner_on_empty_board
        opponent_returns_other_player
        player_opponent_is_involutive
        position_roundtrip
        position_to_index_is_always_valid
        scenario_bankroll_conservation
        scenario_both_natural
        scenario_dealer_bust
        scenario_dealer_natural
        scenario_normal_stand
        scenario_player_bust
        scenario_player_natural
        set_marks_occupied
        soft_hard_exact_relation
        square_equality
        three_card_21_not_blackjack
        verify_bankroll_legos
        verify_blackjack_legos
        verify_card_compositional
        verify_debit_arithmetic
        verify_debit_overdraft_rejected
        verify_debit_zero_bet_rejected
        verify_loss_roundtrip
        verify_no_double_deduction
        verify_outcome_compositional
        verify_push_roundtrip
        verify_rank_compositional
        verify_settle_blackjack
        verify_settle_loss
        verify_settle_push
        verify_settle_surrender
        verify_settle_win
        verify_suit_compositional
        verify_surrender_roundtrip
        verify_tictactoe_compositional
        verify_win_roundtrip
        winner_detects_column
        winner_detects_diagonal
        winner_detects_row
        at_level_clamps
        comeout_classification_exhaustive
        comeout_classification_exclusive
        comeout_craps_three
        comeout_craps_twelve
        comeout_craps_two
        comeout_natural_eleven
        comeout_natural_seven
        craps_numbers_are_not_points
        craps_values_correct
        debit_rejects_over_bankroll
        debit_rejects_zero_bet
        debit_single_bet_correct
        dice_roll_sum_bounded
        die_face_roundtrip
        die_face_value_bounded
        dont_pass_payout_is_even_money
        dont_pass_pushes_on_twelve
        dont_pass_wins_on_craps_two
        house_edge_non_negative
        lesson_advancement_monotonic
        lesson_level_bounded
        natural_values_correct
        pass_line_payout_is_even_money
        pass_line_win_payout_correct
        pass_line_wins_on_natural
        place_five_nine_payout
        place_five_win_payout_correct
        place_four_ten_payout
        place_six_eight_payout
        place_six_win_payout_correct
        point_made_eight
        point_phase_no_decision
        point_roundtrip
        point_values_are_valid
        settle_loss_reduces_balance
        settle_push_returns_wager
        settle_win_returns_correct_balance
        seven_is_not_a_point
        seven_out
        verify_craps_legos
        win_payout_never_zero
    )
    TOTAL=${#HARNESSES[@]}
    echo "🔬 Running $TOTAL Kani harnesses → $CSV"
    echo ""
    for harness in "${HARNESSES[@]}"; do
        printf "  %-50s" "$harness"
        START=$(date +%s)
        if cargo kani --harness "$harness" -p strictly_proofs &>/dev/null; then
            END=$(date +%s)
            ELAPSED=$((END - START))
            echo "kani_proofs,$harness,PASS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "✅ PASS (%ds)\n" "$ELAPSED"
            PASS=$((PASS + 1))
        else
            END=$(date +%s)
            ELAPSED=$((END - START))
            echo "kani_proofs,$harness,FAIL,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "❌ FAIL (%ds)\n" "$ELAPSED"
            FAIL=$((FAIL + 1))
        fi
    done
    echo ""
    echo "Results: $PASS/$TOTAL passed, $FAIL failed"
    echo "CSV:     $CSV"

# Resume a previous tracked run — skips harnesses already recorded as PASS.
verify-kani-resume csv="kani_verification_results.csv":
    #!/usr/bin/env bash
    set -euo pipefail
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then
        echo "No existing CSV at $CSV — use 'just verify-kani-tracked' to start fresh."
        exit 1
    fi
    # Build set of already-passed harnesses
    PASSED=$(awk -F',' '$3=="PASS" {print $2}' "$CSV" | sort -u)
    HARNESSES=(
        ace_ace_nine_value ace_ace_ten_soft_collapses ace_detection ace_raw_value_is_eleven
        blackjack_ace_king blackjack_ace_ten blackjack_biconditional_converse blackjack_requires_two_cards
        bust_detection cannot_split_different_ranks cannot_split_wrong_count can_split_matching_ranks
        card_equality card_value_in_range deal_reduces_remaining deck_all_cards_unique deck_has_52_cards
        double_ace_value empty_hand_zero_value exactly_21_not_bust exhausted_deck_returns_none
        face_card_values_are_ten get_set_roundtrip hand_value_ace_busts_soft hand_value_bounds
        handvalue_equality hand_value_no_aces hand_value_single_ace_soft new_board_is_empty
        no_bust_under_21 no_winner_on_empty_board opponent_returns_other_player player_opponent_is_involutive
        position_roundtrip position_to_index_is_always_valid scenario_bankroll_conservation
        scenario_both_natural scenario_dealer_bust scenario_dealer_natural scenario_normal_stand
        scenario_player_bust scenario_player_natural set_marks_occupied soft_hard_exact_relation
        square_equality three_card_21_not_blackjack verify_bankroll_legos verify_blackjack_legos
        verify_card_compositional verify_debit_arithmetic verify_debit_overdraft_rejected
        verify_debit_zero_bet_rejected verify_loss_roundtrip verify_no_double_deduction
        verify_outcome_compositional verify_push_roundtrip verify_rank_compositional
        verify_settle_blackjack verify_settle_loss verify_settle_push verify_settle_surrender
        verify_settle_win verify_suit_compositional verify_surrender_roundtrip
        verify_tictactoe_compositional verify_win_roundtrip winner_detects_column
        winner_detects_diagonal winner_detects_row
    )
    PASS=0; FAIL=0; SKIP=0
    echo "🔬 Resuming Kani run — skipping already-passed harnesses"
    echo ""
    for harness in "${HARNESSES[@]}"; do
        if echo "$PASSED" | grep -qx "$harness"; then
            printf "  %-50s⏭  SKIP (already passed)\n" "$harness"
            SKIP=$((SKIP + 1))
            continue
        fi
        printf "  %-50s" "$harness"
        START=$(date +%s)
        if cargo kani --harness "$harness" -p strictly_proofs &>/dev/null; then
            END=$(date +%s); ELAPSED=$((END - START))
            echo "kani_proofs,$harness,PASS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "✅ PASS (%ds)\n" "$ELAPSED"
            PASS=$((PASS + 1))
        else
            END=$(date +%s); ELAPSED=$((END - START))
            echo "kani_proofs,$harness,FAIL,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "❌ FAIL (%ds)\n" "$ELAPSED"
            FAIL=$((FAIL + 1))
        fi
    done
    echo ""
    echo "Results: $PASS newly passed, $FAIL failed, $SKIP skipped"
    echo "CSV:     $CSV"

# Print a summary of a previous tracked run from the CSV.
verify-kani-summary csv="kani_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then
        echo "No CSV found at $CSV"
        exit 1
    fi
    PASS=$(awk -F',' '$3=="PASS"' "$CSV" | wc -l | tr -d ' ')
    FAIL=$(awk -F',' '$3=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    TOTAL=$((PASS + FAIL))
    echo "📊 Kani verification summary ($CSV)"
    echo "   Passed: $PASS / $TOTAL"
    echo "   Failed: $FAIL"
    if [ "$FAIL" -gt 0 ]; then
        echo ""
        echo "Failed harnesses:"
        awk -F',' '$3=="FAIL" {printf "  ❌ %s\n", $2}' "$CSV"
    fi

# Show only failed harnesses from the CSV.
verify-kani-failed csv="kani_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then echo "No CSV at $CSV"; exit 1; fi
    FAIL=$(awk -F',' '$3=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    if [ "$FAIL" -eq 0 ]; then
        echo "✅ No failed harnesses in $CSV"
    else
        echo "❌ Failed harnesses:"
        awk -F',' '$3=="FAIL" {printf "  %s  (%ss)\n", $2, $4}' "$CSV"
    fi


    @echo "Running tracked Verus verification..."
    cargo run --bin strictly_games -- verify --tool verus

# Run Creusot verification with CSV tracking
verify-creusot-tracked:
    @echo "Running tracked Creusot verification..."
    cargo run --bin strictly_games -- verify --tool creusot

# Show current verification status from CSV
verify-status csv="kani_verification_results.csv":
    just verify-kani-summary {{csv}}

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
