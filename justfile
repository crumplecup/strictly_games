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
    cargo kani -p strictly_proofs --harness verify_tictactoe_composition_capstone
    cargo kani -p strictly_proofs --harness verify_blackjack_legos
    cargo kani -p strictly_proofs --harness verify_craps_legos

# Run game invariant proofs (game rules correctness)
verify-invariants:
    @echo "Verifying game-specific invariants..."
    cargo kani -p strictly_proofs --harness player_opponent_is_involutive
    cargo kani -p strictly_proofs --harness position_to_index_is_always_valid
    cargo kani -p strictly_proofs --harness is_full_iff_no_empty_squares
    cargo kani -p strictly_proofs --harness is_full_false_on_new_board

# Run TicTacToe wrapper-layer proofs (contracts, typestate, replay, terminal transitions)
verify-tictactoe-contracts:
    @echo "Verifying TicTacToe wrapper layer (contracts + typestate + terminal transitions)..."
    cargo kani -p strictly_proofs \
        --harness validate_square_empty_ok_when_empty \
        --harness validate_square_empty_err_when_occupied \
        --harness validate_player_turn_ok_when_correct_player \
        --harness validate_player_turn_err_when_wrong_player \
        --harness validate_move_ok_on_fresh_game_for_x \
        --harness validate_move_err_occupied_square \
        --harness validate_move_err_wrong_player \
        --harness execute_move_sets_square \
        --harness execute_move_records_history \
        --harness make_move_alternates_player \
        --harness make_move_rejects_wrong_player \
        --harness make_move_rejects_occupied_square \
        --harness replay_empty_gives_fresh_game \
        --harness replay_one_move_applies_it \
        --harness replay_two_moves_alternates_and_records \
        --harness make_move_produces_winner \
        --harness make_move_produces_draw \
        --harness restart_creates_fresh_game

# Run generated kani_proof() foundation harnesses (newtype wrappers and constructibility)
verify-generated:
    @echo "Verifying generated kani_proof() foundation harnesses..."
    cargo kani -p strictly_proofs \
        --harness verify_player_constructible \
        --harness verify_position_constructible \
        --harness verify_board_newtype_wrapper \
        --harness verify_move_newtype_wrapper \
        --harness verify_gamesetup_newtype_wrapper \
        --harness verify_gameinprogress_newtype_wrapper \
        --harness verify_gamefinished_newtype_wrapper \
        --harness verify_rank_constructible \
        --harness verify_suit_constructible \
        --harness verify_card_newtype_wrapper \
        --harness verify_outcome_constructible

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

# Run TUI breakpoint truth-table proofs (NoOverflow layout arithmetic) — Kani
verify-tui-breakpoints:
    @echo "Verifying TUI layout contracts across all 7 terminal breakpoints..."
    cargo kani -p strictly_proofs --harness truncation_always_satisfies_label_contained
    cargo kani -p strictly_proofs --harness node_box_width_no_u16_overflow
    cargo kani -p strictly_proofs --harness area_sufficient_zero_height_fails
    cargo kani -p strictly_proofs --harness area_sufficient_nonzero_height_passes
    cargo kani -p strictly_proofs --harness breakpoint_minimum_blackjack_layout
    cargo kani -p strictly_proofs --harness breakpoint_small_layout
    cargo kani -p strictly_proofs --harness breakpoint_medium_layout
    cargo kani -p strictly_proofs --harness breakpoint_large_layout
    cargo kani -p strictly_proofs --harness breakpoint_ultrawide_layout
    cargo kani -p strictly_proofs --harness breakpoint_tiny_graceful_degrade
    cargo kani -p strictly_proofs --harness breakpoint_micro_expected_failure
    cargo kani -p strictly_proofs --harness symbolic_must_pass_range_safe

# Run TUI breakpoint proofs — Creusot (Why3 deductive verification)
# Requires: cargo install cargo-creusot
verify-tui-breakpoints-creusot:
    @echo "Verifying TUI layout contracts with Creusot (Why3)..."
    @echo "Properties: truncation_output_bounded, truncation_identity,"
    @echo "            truncation_satisfies_label_contained (universal),"
    @echo "            node_box_no_overflow, area_sufficient checks,"
    @echo "            breakpoint witnesses (minimum, micro, tiny)"
    cargo creusot -p strictly_proofs

# Run TUI breakpoint proofs — Verus (Z3 SMT specification-based)
# Requires: verus binary on PATH  (see crates/strictly_proofs/src/verus_proofs/README.md)
verify-tui-breakpoints-verus:
    @echo "Verifying TUI layout contracts with Verus (Z3)..."
    @echo "Properties: truncation_output_bounded, truncation_identity,"
    @echo "            truncation_always_satisfies_label_contained (universal),"
    @echo "            node_box_no_overflow, area_sufficient_{fails,passes},"
    @echo "            breakpoint_{minimum,ultrawide,micro,tiny}, symbolic_must_pass_range"
    verus --crate-type=lib crates/strictly_proofs/src/verus_proofs/tui_breakpoints.rs

# Run TUI breakpoint proofs with all three verifiers (Kani + Creusot + Verus)
verify-tui-breakpoints-all:
    just verify-tui-breakpoints
    just verify-tui-breakpoints-creusot
    just verify-tui-breakpoints-verus

# Check that verification code compiles (fast check before running Kani)
verify-check:
    @echo "Checking verification code compiles..."
    cargo check -p strictly_proofs

# Run each Kani harness individually, recording pass/fail/duration to a CSV.
# Format: module,harness,status,duration_secs,timestamp
# Usage: just verify-kani-tracked                  (fresh run, overwrites CSV)
#        just verify-kani-tracked my.csv           (custom CSV path)
#        just verify-kani-tracked my.csv 600       (custom timeout)
verify-kani-tracked csv="kani_verification_results.csv" timeout="300":
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
        area_sufficient_nonzero_height_passes
        area_sufficient_zero_height_fails
        at_level_clamps
        blackjack_ace_king
        blackjack_ace_ten
        blackjack_biconditional_converse
        blackjack_requires_two_cards
        breakpoint_large_layout
        breakpoint_medium_layout
        breakpoint_micro_expected_failure
        breakpoint_minimum_blackjack_layout
        breakpoint_small_layout
        breakpoint_tiny_graceful_degrade
        breakpoint_ultrawide_layout
        bust_detection
        cannot_split_different_ranks
        cannot_split_wrong_count
        can_split_matching_ranks
        card_equality
        card_value_in_range
        comeout_classification_exclusive
        comeout_classification_exhaustive
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
        double_ace_value
        empty_hand_zero_value
        exactly_21_not_bust
        execute_move_records_history
        execute_move_sets_square
        exhausted_shoe_returns_none
        face_card_values_are_ten
        generate_reduces_remaining
        get_set_roundtrip
        hand_value_ace_busts_soft
        hand_value_bounds
        handvalue_equality
        hand_value_no_aces
        hand_value_single_ace_soft
        house_edge_non_negative
        is_full_false_on_new_board
        is_full_iff_no_empty_squares
        lesson_advancement_monotonic
        lesson_level_bounded
        make_move_alternates_player
        make_move_produces_draw
        make_move_produces_winner
        make_move_rejects_occupied_square
        make_move_rejects_wrong_player
        natural_values_correct
        new_board_is_empty
        no_bust_under_21
        node_box_width_no_u16_overflow
        no_winner_on_empty_board
        opponent_returns_other_player
        pass_line_payout_is_even_money
        pass_line_win_payout_correct
        pass_line_wins_on_natural
        place_five_nine_payout
        place_five_win_payout_correct
        place_four_ten_payout
        place_six_eight_payout
        place_six_win_payout_correct
        player_opponent_is_involutive
        point_made_eight
        point_phase_no_decision
        point_roundtrip
        point_values_are_valid
        position_roundtrip
        position_to_index_is_always_valid
        replay_empty_gives_fresh_game
        replay_one_move_applies_it
        replay_two_moves_alternates_and_records
        restart_creates_fresh_game
        scenario_bankroll_conservation
        scenario_both_natural
        scenario_dealer_bust
        scenario_dealer_natural
        scenario_normal_stand
        scenario_player_bust
        scenario_player_natural
        set_marks_occupied
        settle_loss_reduces_balance
        settle_push_returns_wager
        settle_win_returns_correct_balance
        seven_is_not_a_point
        seven_out
        shoe_all_cards_unique
        shoe_has_52_cards
        soft_hard_exact_relation
        square_equality
        symbolic_must_pass_range_safe
        three_card_21_not_blackjack
        truncation_always_satisfies_label_contained
        validate_move_err_occupied_square
        validate_move_err_wrong_player
        validate_move_ok_on_fresh_game_for_x
        validate_player_turn_err_when_wrong_player
        validate_player_turn_ok_when_correct_player
        validate_square_empty_err_when_occupied
        validate_square_empty_ok_when_empty
        verify_bankroll_legos
        verify_blackjack_legos
        verify_board_newtype_wrapper
        verify_card_newtype_wrapper
        verify_craps_legos
        verify_debit_arithmetic
        verify_debit_overdraft_rejected
        verify_debit_zero_bet_rejected
        verify_gamefinished_newtype_wrapper
        verify_gameinprogress_newtype_wrapper
        verify_gamesetup_newtype_wrapper
        verify_loss_roundtrip
        verify_move_newtype_wrapper
        verify_no_double_deduction
        verify_outcome_constructible
        verify_player_constructible
        verify_position_constructible
        verify_push_roundtrip
        verify_rank_constructible
        verify_settle_blackjack
        verify_settle_loss
        verify_settle_push
        verify_settle_surrender
        verify_settle_win
        verify_suit_constructible
        verify_surrender_roundtrip
        verify_tictactoe_composition_capstone
        verify_win_roundtrip
        winner_detects_column
        winner_detects_diagonal
        winner_detects_row
        win_payout_never_zero
    )
    TOTAL=${#HARNESSES[@]}
    echo "🔬 Running $TOTAL Kani harnesses → $CSV"
    echo ""
    IDX=0
    for harness in "${HARNESSES[@]}"; do
        IDX=$((IDX + 1))
        printf "  [%d/%d] %-50s" "$IDX" "$TOTAL" "$harness"
        START=$(date +%s)
        if timeout "{{timeout}}" cargo kani --harness "$harness" -p strictly_proofs &>/dev/null; then
            END=$(date +%s)
            ELAPSED=$((END - START))
            echo "kani_proofs,$harness,PASS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "✅ PASS (%ds)\n" "$ELAPSED"
            PASS=$((PASS + 1))
        else
            END=$(date +%s)
            ELAPSED=$((END - START))
            STATUS=$( [ $ELAPSED -ge {{timeout}} ] && echo "TIMEOUT" || echo "FAIL" )
            echo "kani_proofs,$harness,$STATUS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            [ "$STATUS" = "TIMEOUT" ] && printf "⏱  TIMEOUT (%ds)\n" "$ELAPSED" || printf "❌ FAIL (%ds)\n" "$ELAPSED"
            FAIL=$((FAIL + 1))
        fi
    done
    echo ""
    echo "Results: $PASS/$TOTAL passed, $FAIL failed"
    echo "CSV:     $CSV"

# Resume a previous tracked run — skips harnesses already recorded as PASS.
verify-kani-resume csv="kani_verification_results.csv" timeout="300":
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
        ace_ace_nine_value
        ace_ace_ten_soft_collapses
        ace_detection
        ace_raw_value_is_eleven
        area_sufficient_nonzero_height_passes
        area_sufficient_zero_height_fails
        at_level_clamps
        blackjack_ace_king
        blackjack_ace_ten
        blackjack_biconditional_converse
        blackjack_requires_two_cards
        breakpoint_large_layout
        breakpoint_medium_layout
        breakpoint_micro_expected_failure
        breakpoint_minimum_blackjack_layout
        breakpoint_small_layout
        breakpoint_tiny_graceful_degrade
        breakpoint_ultrawide_layout
        bust_detection
        cannot_split_different_ranks
        cannot_split_wrong_count
        can_split_matching_ranks
        card_equality
        card_value_in_range
        comeout_classification_exclusive
        comeout_classification_exhaustive
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
        double_ace_value
        empty_hand_zero_value
        exactly_21_not_bust
        execute_move_records_history
        execute_move_sets_square
        exhausted_shoe_returns_none
        face_card_values_are_ten
        generate_reduces_remaining
        get_set_roundtrip
        hand_value_ace_busts_soft
        hand_value_bounds
        handvalue_equality
        hand_value_no_aces
        hand_value_single_ace_soft
        house_edge_non_negative
        is_full_false_on_new_board
        is_full_iff_no_empty_squares
        lesson_advancement_monotonic
        lesson_level_bounded
        make_move_alternates_player
        make_move_produces_draw
        make_move_produces_winner
        make_move_rejects_occupied_square
        make_move_rejects_wrong_player
        natural_values_correct
        new_board_is_empty
        no_bust_under_21
        node_box_width_no_u16_overflow
        no_winner_on_empty_board
        opponent_returns_other_player
        pass_line_payout_is_even_money
        pass_line_win_payout_correct
        pass_line_wins_on_natural
        place_five_nine_payout
        place_five_win_payout_correct
        place_four_ten_payout
        place_six_eight_payout
        place_six_win_payout_correct
        player_opponent_is_involutive
        point_made_eight
        point_phase_no_decision
        point_roundtrip
        point_values_are_valid
        position_roundtrip
        position_to_index_is_always_valid
        replay_empty_gives_fresh_game
        replay_one_move_applies_it
        replay_two_moves_alternates_and_records
        restart_creates_fresh_game
        scenario_bankroll_conservation
        scenario_both_natural
        scenario_dealer_bust
        scenario_dealer_natural
        scenario_normal_stand
        scenario_player_bust
        scenario_player_natural
        set_marks_occupied
        settle_loss_reduces_balance
        settle_push_returns_wager
        settle_win_returns_correct_balance
        seven_is_not_a_point
        seven_out
        shoe_all_cards_unique
        shoe_has_52_cards
        soft_hard_exact_relation
        square_equality
        symbolic_must_pass_range_safe
        three_card_21_not_blackjack
        truncation_always_satisfies_label_contained
        validate_move_err_occupied_square
        validate_move_err_wrong_player
        validate_move_ok_on_fresh_game_for_x
        validate_player_turn_err_when_wrong_player
        validate_player_turn_ok_when_correct_player
        validate_square_empty_err_when_occupied
        validate_square_empty_ok_when_empty
        verify_bankroll_legos
        verify_blackjack_legos
        verify_board_newtype_wrapper
        verify_card_newtype_wrapper
        verify_craps_legos
        verify_debit_arithmetic
        verify_debit_overdraft_rejected
        verify_debit_zero_bet_rejected
        verify_gamefinished_newtype_wrapper
        verify_gameinprogress_newtype_wrapper
        verify_gamesetup_newtype_wrapper
        verify_loss_roundtrip
        verify_move_newtype_wrapper
        verify_no_double_deduction
        verify_outcome_constructible
        verify_player_constructible
        verify_position_constructible
        verify_push_roundtrip
        verify_rank_constructible
        verify_settle_blackjack
        verify_settle_loss
        verify_settle_push
        verify_settle_surrender
        verify_settle_win
        verify_suit_constructible
        verify_surrender_roundtrip
        verify_tictactoe_composition_capstone
        verify_win_roundtrip
        winner_detects_column
        winner_detects_diagonal
        winner_detects_row
        win_payout_never_zero
    )
    PASS=0; FAIL=0; SKIP=0
    TOTAL=${#HARNESSES[@]}
    echo "🔬 Resuming Kani run — skipping already-passed harnesses ($TOTAL total)"
    echo ""
    IDX=0
    for harness in "${HARNESSES[@]}"; do
        IDX=$((IDX + 1))
        if echo "$PASSED" | grep -qx "$harness"; then
            printf "  [%d/%d] %-50s⏭  SKIP (already passed)\n" "$IDX" "$TOTAL" "$harness"
            SKIP=$((SKIP + 1))
            continue
        fi
        printf "  [%d/%d] %-50s" "$IDX" "$TOTAL" "$harness"
        START=$(date +%s)
        if timeout "{{timeout}}" cargo kani --harness "$harness" -p strictly_proofs &>/dev/null; then
            END=$(date +%s); ELAPSED=$((END - START))
            echo "kani_proofs,$harness,PASS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            printf "✅ PASS (%ds)\n" "$ELAPSED"
            PASS=$((PASS + 1))
        else
            END=$(date +%s); ELAPSED=$((END - START))
            STATUS=$( [ $ELAPSED -ge {{timeout}} ] && echo "TIMEOUT" || echo "FAIL" )
            echo "kani_proofs,$harness,$STATUS,$ELAPSED,$(date -Iseconds)" >> "$CSV"
            [ "$STATUS" = "TIMEOUT" ] && printf "⏱  TIMEOUT (%ds)\n" "$ELAPSED" || printf "❌ FAIL (%ds)\n" "$ELAPSED"
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
    TIMEOUT=$(awk -F',' '$3=="TIMEOUT"' "$CSV" | wc -l | tr -d ' ')
    TOTAL=$((PASS + FAIL + TIMEOUT))
    echo "📊 Kani verification summary ($CSV)"
    echo "   Passed:  $PASS / $TOTAL"
    echo "   Failed:  $FAIL"
    [ "$TIMEOUT" -gt 0 ] && echo "   Timeout: $TIMEOUT"
    if [ "$FAIL" -gt 0 ]; then
        echo ""
        echo "Failed harnesses:"
        awk -F',' '$3=="FAIL" {printf "  ❌ %s\n", $2}' "$CSV"
    fi
    if [ "$TIMEOUT" -gt 0 ]; then
        echo ""
        echo "Timed-out harnesses:"
        awk -F',' '$3=="TIMEOUT" {printf "  ⏱  %s  (%ss)\n", $2, $4}' "$CSV"
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


# ─────────────────────────────────────────────────────────────
# Verus verification tracking
# CSV format: module,status,verified,errors,duration_secs,timestamp
# ─────────────────────────────────────────────────────────────

# Run Verus verification with CSV tracking (recommended)
# Usage: just verify-verus-tracked                  (fresh run)
#        just verify-verus-tracked my.csv           (custom CSV)
#        just verify-verus-tracked my.csv 300       (custom timeout)
verify-verus-tracked csv="verus_verification_results.csv" timeout="600":
    #!/usr/bin/env bash
    set -euo pipefail

    RAW_VERUS=$(grep -v '^#' .env 2>/dev/null | grep '^VERUS_PATH=' | sed 's/^VERUS_PATH=//' | tr -d '"' || true)
    VERUS_BIN="${RAW_VERUS/\~/$HOME}"
    if [ -z "$VERUS_BIN" ] || [ ! -f "$VERUS_BIN" ]; then
        echo "❌ Verus not found at: '${VERUS_BIN}'"
        echo "   Set VERUS_PATH in .env"
        exit 1
    fi

    CSV="{{csv}}"
    echo "module,status,verified,errors,duration_secs,timestamp" > "$CSV"
    PASS=0; FAIL=0

    # Generate verus_proof() composed foundation files before verifying
    cargo build -p strictly_proofs --quiet 2>/dev/null || true

    for file in crates/strictly_proofs/src/verus_proofs/*.rs crates/strictly_proofs/src/verus_proofs/generated/*.rs; do
        [[ -f "$file" ]] || continue
        module=$(basename "$file" .rs)
        [[ "$module" == "mod" ]] && continue

        echo -n "  🔬 $module ... "
        START=$(date +%s%3N)
        OUTPUT=$(timeout "{{timeout}}" "$VERUS_BIN" --crate-type=lib "$file" 2>&1) || true
        END=$(date +%s%3N)
        ELAPSED=$(( (END - START) / 1000 ))

        VERIFIED=$(echo "$OUTPUT" | grep -oP '\d+(?= verified)' | tail -1 || echo "0")
        ERRORS=$(echo "$OUTPUT" | grep -oP '\d+(?= error)' | tail -1 || echo "0")
        TS=$(date -Iseconds)

        if [[ "${ERRORS:-0}" == "0" ]] && echo "$OUTPUT" | grep -q "verified"; then
            STATUS="PASS"; PASS=$((PASS + 1))
            echo "✅  ($VERIFIED verified, ${ELAPSED}s)"
        else
            STATUS="FAIL"; FAIL=$((FAIL + 1))
            echo "❌  ($ERRORS errors, ${ELAPSED}s)"
        fi
        echo "$module,$STATUS,$VERIFIED,$ERRORS,$ELAPSED,$TS" >> "$CSV"
    done

    echo ""
    echo "Results: $PASS passed, $FAIL failed"
    echo "CSV:     $CSV"
    [ "$FAIL" -eq 0 ] || exit 1

# Resume Verus tracking — skip modules already marked PASS in CSV
verify-verus-resume csv="verus_verification_results.csv" timeout="600":
    #!/usr/bin/env bash
    set -euo pipefail

    RAW_VERUS=$(grep -v '^#' .env 2>/dev/null | grep '^VERUS_PATH=' | sed 's/^VERUS_PATH=//' | tr -d '"' || true)
    VERUS_BIN="${RAW_VERUS/\~/$HOME}"
    if [ -z "$VERUS_BIN" ] || [ ! -f "$VERUS_BIN" ]; then
        echo "❌ Verus not found at: '${VERUS_BIN}'. Set VERUS_PATH in .env"; exit 1
    fi

    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then
        echo "No CSV at $CSV — use 'just verify-verus-tracked' to start fresh."
        exit 1
    fi

    PASS=0; FAIL=0; SKIP=0

    # Ensure verus_proof() composed foundation files are up to date
    cargo build -p strictly_proofs --quiet 2>/dev/null || true

    for file in crates/strictly_proofs/src/verus_proofs/*.rs crates/strictly_proofs/src/verus_proofs/generated/*.rs; do
        [[ -f "$file" ]] || continue
        module=$(basename "$file" .rs)
        [[ "$module" == "mod" ]] && continue

        if grep -q "^$module,PASS," "$CSV" 2>/dev/null; then
            echo "  ⏭  $module (already PASS — skipping)"
            SKIP=$((SKIP + 1)); continue
        fi

        echo -n "  🔬 $module ... "
        START=$(date +%s%3N)
        OUTPUT=$(timeout "{{timeout}}" "$VERUS_BIN" --crate-type=lib "$file" 2>&1) || true
        END=$(date +%s%3N)
        ELAPSED=$(( (END - START) / 1000 ))

        VERIFIED=$(echo "$OUTPUT" | grep -oP '\d+(?= verified)' | tail -1 || echo "0")
        ERRORS=$(echo "$OUTPUT" | grep -oP '\d+(?= error)' | tail -1 || echo "0")
        TS=$(date -Iseconds)

        if [[ "${ERRORS:-0}" == "0" ]] && echo "$OUTPUT" | grep -q "verified"; then
            STATUS="PASS"; PASS=$((PASS + 1))
            echo "✅  ($VERIFIED verified, ${ELAPSED}s)"
        else
            STATUS="FAIL"; FAIL=$((FAIL + 1))
            echo "❌  ($ERRORS errors, ${ELAPSED}s)"
        fi
        # Update or append the row
        sed -i "/^$module,/d" "$CSV" 2>/dev/null || true
        echo "$module,$STATUS,$VERIFIED,$ERRORS,$ELAPSED,$TS" >> "$CSV"
    done

    echo ""
    echo "Results: $PASS newly passed, $FAIL failed, $SKIP skipped"
    echo "CSV:     $CSV"

# Show Verus verification summary from CSV
verify-verus-summary csv="verus_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then echo "No CSV at $CSV"; exit 1; fi
    PASS=$(awk -F',' '$2=="PASS"' "$CSV" | wc -l | tr -d ' ')
    FAIL=$(awk -F',' '$2=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    TOTAL=$((PASS + FAIL))
    echo "📊 Verus verification summary ($CSV)"
    echo "   Passed: $PASS / $TOTAL"
    echo "   Failed: $FAIL"
    if [ "$FAIL" -gt 0 ]; then
        echo ""
        echo "Failed modules:"
        awk -F',' '$2=="FAIL" {printf "  ❌ %s\n", $1}' "$CSV"
    fi

# Show only failed Verus modules from CSV
verify-verus-failed csv="verus_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then echo "No CSV at $CSV"; exit 1; fi
    FAIL=$(awk -F',' '$2=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    if [ "$FAIL" -eq 0 ]; then
        echo "✅ No failed modules in $CSV"
    else
        echo "❌ Failed modules:"
        awk -F',' '$2=="FAIL" {printf "  %s  (%ss)\n", $1, $5}' "$CSV"
    fi

# List all Verus proof modules
verify-verus-list:
    @echo "Verus proof modules:"
    @for f in crates/strictly_proofs/src/verus_proofs/*.rs; do \
        m=$(basename "$f" .rs); \
        [ "$m" != "mod" ] && echo "  $m"; \
    done

# ─────────────────────────────────────────────────────────────
# Creusot verification tracking
# Module CSV format: module,status,duration_secs,timestamp
# Goals CSV format:  module,function,goal,status,duration_secs,timestamp
# ─────────────────────────────────────────────────────────────

# Run Creusot verification with CSV tracking (cargo check, one row per module)
# Usage: just verify-creusot-tracked                (fresh run)
#        just verify-creusot-tracked my.csv         (custom CSV)
verify-creusot-tracked csv="creusot_verification_results.csv":
    #!/usr/bin/env bash
    set -euo pipefail

    if ! opam exec -- cargo creusot version &>/dev/null 2>&1; then
        echo "❌ cargo-creusot not found via opam."
        echo "   Install: opam install creusot  (or cargo install cargo-creusot)"
        exit 1
    fi

    CSV="{{csv}}"
    echo "module,status,duration_secs,timestamp" > "$CSV"
    PASS=0; FAIL=0
    MODULES=(bankroll_financial compositional_proof game_invariants tui_breakpoints)

    for module in "${MODULES[@]}"; do
        echo -n "  🔬 $module ... "
        START=$(date +%s%3N)
        # cargo creusot compiles with creusot cfg and checks contracts
        OUTPUT=$(opam exec -- cargo creusot -- -p strictly_proofs 2>&1) || RC=$?
        END=$(date +%s%3N)
        ELAPSED=$(( (END - START) / 1000 ))
        TS=$(date -Iseconds)

        # If the full package check passed we mark this module as PASS;
        # subsequent modules in the same run reuse the cached build result.
        if [ "${RC:-0}" -eq 0 ]; then
            STATUS="PASS"; PASS=$((PASS + 1))
            echo "✅  (${ELAPSED}s)"
        else
            STATUS="FAIL"; FAIL=$((FAIL + 1))
            echo "❌  (${ELAPSED}s)"
            echo "    $(echo "$OUTPUT" | grep -E 'error|warning' | head -3)"
        fi
        echo "$module,$STATUS,$ELAPSED,$TS" >> "$CSV"
        RC=0  # reset for next iteration
    done

    echo ""
    echo "Results: $PASS passed, $FAIL failed"
    echo "CSV:     $CSV"
    [ "$FAIL" -eq 0 ] || exit 1

# Run Creusot SMT prove pass and track per-goal results
# Requires: cargo-creusot + why3find + SMT solvers (z3, alt-ergo, cvc5)
# Usage: just verify-creusot-prove
#        just verify-creusot-prove my_modules.csv my_goals.csv
verify-creusot-prove csv="creusot_module_results.csv" goals="creusot_goal_results.csv":
    #!/usr/bin/env bash
    set -euo pipefail

    if ! opam exec -- why3find --version &>/dev/null 2>&1; then
        echo "❌ why3find not found via opam. Install: opam install why3find"
        exit 1
    fi

    echo "🔬 Running Creusot prove pass (cargo creusot prove)..."
    echo "   Module CSV: {{csv}}"
    echo "   Goals CSV:  {{goals}}"
    echo ""

    MODULE_CSV="{{csv}}"
    GOALS_CSV="{{goals}}"
    echo "module,status,duration_secs,timestamp" > "$MODULE_CSV"
    echo "module,function,goal,status,prover,duration_secs,timestamp" > "$GOALS_CSV"

    PASS=0; FAIL=0; GOALS_PROVED=0; GOALS_TOTAL=0
    MODULES=(bankroll_financial compositional_proof game_invariants tui_breakpoints)

    for module in "${MODULES[@]}"; do
        echo "  📐 $module"
        START=$(date +%s%3N)
        OUTPUT=$(opam exec -- cargo creusot prove -- -p strictly_proofs 2>&1) || RC=$?
        END=$(date +%s%3N)
        ELAPSED=$(( (END - START) / 1000 ))
        TS=$(date -Iseconds)

        MODULE_STATUS="PASS"
        # Parse why3find output lines: look for Valid/Timeout/Unknown per goal
        while IFS= read -r line; do
            # why3find outputs: "  function_name: Valid (prover, Xs)"
            if echo "$line" | grep -qE 'Valid|Timeout|Unknown|Failed'; then
                GOAL_STATUS=$(echo "$line" | grep -oE 'Valid|Timeout|Unknown|Failed' | head -1)
                PROVER=$(echo "$line" | grep -oP '(?<=\()[\w@.]+(?=,)' || echo "unknown")
                GOAL_ELAPSED=$(echo "$line" | grep -oP '[\d.]+(?=s\))' || echo "0")
                FUNC=$(echo "$line" | grep -oP '^\s+\K\S+(?=:)' || echo "unknown")
                GOAL_NUM="${GOALS_TOTAL}"
                GOALS_TOTAL=$((GOALS_TOTAL + 1))
                echo "$module,$FUNC,vc${GOAL_NUM},$GOAL_STATUS,$PROVER,$GOAL_ELAPSED,$TS" >> "$GOALS_CSV"
                if [ "$GOAL_STATUS" = "Valid" ]; then
                    GOALS_PROVED=$((GOALS_PROVED + 1))
                    echo "    ✅ $FUNC: $GOAL_STATUS ($PROVER, ${GOAL_ELAPSED}s)"
                else
                    MODULE_STATUS="FAIL"
                    echo "    ❌ $FUNC: $GOAL_STATUS ($PROVER, ${GOAL_ELAPSED}s)"
                fi
            fi
        done <<< "$OUTPUT"

        if [ "$MODULE_STATUS" = "PASS" ]; then
            PASS=$((PASS + 1))
        else
            FAIL=$((FAIL + 1))
        fi
        echo "$module,$MODULE_STATUS,$ELAPSED,$TS" >> "$MODULE_CSV"
        RC=0
    done

    echo ""
    echo "Goals:   $GOALS_PROVED / $GOALS_TOTAL proved"
    echo "Modules: $PASS passed, $FAIL failed"
    echo "Module CSV: $MODULE_CSV"
    echo "Goals  CSV: $GOALS_CSV"
    [ "$FAIL" -eq 0 ] || exit 1

# Show Creusot module-level summary from CSV
verify-creusot-summary csv="creusot_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then echo "No CSV at $CSV"; exit 1; fi
    PASS=$(awk -F',' '$2=="PASS"' "$CSV" | wc -l | tr -d ' ')
    FAIL=$(awk -F',' '$2=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    TOTAL=$((PASS + FAIL))
    echo "📊 Creusot verification summary ($CSV)"
    echo "   Passed: $PASS / $TOTAL"
    echo "   Failed: $FAIL"
    if [ "$FAIL" -gt 0 ]; then
        echo ""
        echo "Failed modules:"
        awk -F',' '$2=="FAIL" {printf "  ❌ %s\n", $1}' "$CSV"
    fi

# Show failed Creusot modules from CSV
verify-creusot-failed csv="creusot_verification_results.csv":
    #!/usr/bin/env bash
    CSV="{{csv}}"
    if [ ! -f "$CSV" ]; then echo "No CSV at $CSV"; exit 1; fi
    FAIL=$(awk -F',' '$2=="FAIL"' "$CSV" | wc -l | tr -d ' ')
    if [ "$FAIL" -eq 0 ]; then
        echo "✅ No failed modules in $CSV"
    else
        echo "❌ Failed modules:"
        awk -F',' '$2=="FAIL" {printf "  %s  (%ss)\n", $1, $3}' "$CSV"
    fi

# Show goal-level summary from Creusot prove CSV
verify-creusot-goal-summary goals="creusot_goal_results.csv":
    #!/usr/bin/env python3
    import csv, sys
    goals_file = "{{goals}}"
    try:
        rows = list(csv.DictReader(open(goals_file)))
    except FileNotFoundError:
        print(f"No goals CSV at {goals_file} — run 'just verify-creusot-prove' first")
        sys.exit(1)
    proved  = sum(1 for r in rows if r.get('status') == 'Valid')
    total   = len(rows)
    modules = sorted(set(r['module'] for r in rows))
    print(f'Goals: {proved}/{total} proved across {len(modules)} modules')
    for m in modules:
        mr = [r for r in rows if r['module'] == m]
        mp = sum(1 for r in mr if r.get('status') == 'Valid')
        print(f'  {m}: {mp}/{len(mr)}')

# List all Creusot proof modules
verify-creusot-list:
    @echo "Creusot proof modules:"
    @for f in crates/strictly_proofs/src/creusot_proofs/*.rs; do \
        m=$(basename "$f" .rs); \
        [ "$m" != "mod" ] && echo "  $m"; \
    done

# Show current verification status — all three tools
verify-status kani_csv="kani_verification_results.csv" verus_csv="verus_verification_results.csv" creusot_csv="creusot_verification_results.csv":
    just verify-kani-summary {{kani_csv}}
    @echo ""
    -just verify-verus-summary {{verus_csv}}
    @echo ""
    -just verify-creusot-summary {{creusot_csv}}

# Run all tracked verification — Kani + Verus + Creusot
verify-all-tracked:
    @echo "🔬 Running verification trifecta (Kani + Verus + Creusot)..."
    just verify-kani-tracked
    just verify-verus-tracked
    just verify-creusot-tracked
    @echo ""
    @echo "✅ Verification trifecta complete."
    just verify-status

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
