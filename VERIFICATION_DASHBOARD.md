# Formal Verification Dashboard

**Last Updated**: 2026-03-29

## Summary

| Metric | Count |
| -------- | ------- |
| **Total Kani Harnesses** | **111** |
| **Blackjack** | 56 |
| **Craps** | 42 |
| **Tic-Tac-Toe** | 13 |
| **Creusot Functions** | ~10 |
| **Verus Proof Functions** | ~9 |

## Coverage by Game

### 🃏 Blackjack (56 harnesses)

Blackjack verification covers card arithmetic, hand classification, game scenarios,
compositional type proofs, and financial settlement integrity.

**Invariants** — `blackjack_invariants.rs` (30 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `ace_ace_nine_value` | Two aces + 9 = soft 21, one ace collapses |
| `ace_ace_ten_soft_collapses` | Two aces + 10 collapses both aces to hard |
| `ace_detection` | Ace rank detected correctly |
| `ace_raw_value_is_eleven` | Ace raw value is 11 |
| `blackjack_ace_king` | Ace + King = natural blackjack |
| `blackjack_ace_ten` | Ace + 10 = natural blackjack |
| `blackjack_biconditional_converse` | Blackjack ⟺ two cards summing to 21 |
| `blackjack_requires_two_cards` | Blackjack requires exactly two cards |
| `bust_detection` | Hand value > 21 correctly detected as bust |
| `cannot_split_different_ranks` | Split rejected for mismatched ranks |
| `cannot_split_wrong_count` | Split rejected with != 2 cards |
| `can_split_matching_ranks` | Split allowed for matching ranks |
| `card_equality` | Card equality comparison correct |
| `card_value_in_range` | Card values ∈ [2, 11] |
| `deal_reduces_remaining` | Dealing reduces deck size by 1 |
| `deck_all_cards_unique` | Standard deck has no duplicates |
| `deck_has_52_cards` | Standard deck has exactly 52 cards |
| `double_ace_value` | Two aces = 12 (one collapses) |
| `empty_hand_zero_value` | Empty hand has value 0 |
| `exactly_21_not_bust` | Hand value = 21 is not bust |
| `exhausted_deck_returns_none` | Empty deck returns None on deal |
| `face_card_values_are_ten` | J/Q/K all have value 10 |
| `hand_value_ace_busts_soft` | Ace collapses when soft hand would bust |
| `hand_value_bounds` | Hand value bounded by cards × 11 |
| `hand_value_no_aces` | Non-ace hands sum raw values |
| `hand_value_single_ace_soft` | Single ace keeps hand soft when ≤ 21 |
| `handvalue_equality` | HandValue equality comparison |
| `no_bust_under_21` | Values < 21 never bust |
| `soft_hard_exact_relation` | Soft value = hard value + 10 |
| `three_card_21_not_blackjack` | 3-card 21 is not natural blackjack |

**Scenarios** — `blackjack_scenarios.rs` (7 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `scenario_bankroll_conservation` | Win/loss/push conserve total money |
| `scenario_both_natural` | Player + dealer natural = push |
| `scenario_dealer_bust` | Dealer bust = player win |
| `scenario_dealer_natural` | Dealer natural = player loss |
| `scenario_normal_stand` | Standing compares values correctly |
| `scenario_player_bust` | Player bust = loss (regardless of dealer) |
| `scenario_player_natural` | Player natural = blackjack payout |

**Compositional** — `blackjack_compositional.rs` (5 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `verify_blackjack_legos` | All blackjack types pass `kani_proof()` |
| `verify_card_compositional` | Card type compositional proof |
| `verify_outcome_compositional` | Outcome type compositional proof |
| `verify_rank_compositional` | Rank type compositional proof |
| `verify_suit_compositional` | Suit type compositional proof |

**Financial** — `bankroll_financial.rs` (14 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `verify_bankroll_legos` | All bankroll types pass `kani_proof()` |
| `verify_debit_arithmetic` | Debit correctly reduces balance |
| `verify_debit_overdraft_rejected` | Overdraft attempt returns error |
| `verify_debit_zero_bet_rejected` | Zero bet rejected |
| `verify_loss_roundtrip` | Loss: final = initial − bet |
| `verify_no_double_deduction` | Debit token consumed exactly once |
| `verify_push_roundtrip` | Push: final = initial |
| `verify_settle_blackjack` | Blackjack pays 3:2 |
| `verify_settle_loss` | Loss settles correctly |
| `verify_settle_push` | Push returns wager |
| `verify_settle_surrender` | Surrender returns half |
| `verify_settle_win` | Win pays even money |
| `verify_surrender_roundtrip` | Surrender: final = initial − ⌊bet/2⌋ |
| `verify_win_roundtrip` | Win: final = initial + bet |

### 🎲 Craps (42 harnesses)

Craps verification covers dice arithmetic, point classification, come-out
exhaustiveness, payout ratios, game scenario correctness, financial settlement,
and progressive lesson bounds.

**Invariants** — `craps_invariants.rs` (17 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `die_face_value_bounded` | DieFace::value() ∈ {1..6} |
| `dice_roll_sum_bounded` | DiceRoll::sum() ∈ {2..12} |
| `die_face_roundtrip` | from_value(value(f)) = Some(f) |
| `point_values_are_valid` | Point values ∈ {4, 5, 6, 8, 9, 10} |
| `point_roundtrip` | from_sum(value(p)) = Some(p) |
| `seven_is_not_a_point` | from_sum(7) = None |
| `craps_numbers_are_not_points` | from_sum(2/3/11/12) = None |
| `comeout_classification_exhaustive` | Every roll is natural ∨ craps ∨ point |
| `comeout_classification_exclusive` | Exactly one classification per roll |
| `natural_values_correct` | is_natural ⟺ sum ∈ {7, 11} |
| `craps_values_correct` | is_craps ⟺ sum ∈ {2, 3, 12} |
| `pass_line_payout_is_even_money` | payout_ratio(PassLine) = (1, 1) |
| `dont_pass_payout_is_even_money` | payout_ratio(DontPass) = (1, 1) |
| `place_six_eight_payout` | Place 6/8 pay 7:6 |
| `place_five_nine_payout` | Place 5/9 pay 7:5 |
| `place_four_ten_payout` | Place 4/10 pay 9:5 |
| `house_edge_non_negative` | ∀ bet, house_edge(bet) ≥ 0 |

**Scenarios** — `craps_scenarios.rs` (11 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `comeout_natural_seven` | Roll 7 on come-out → resolved, pass wins |
| `comeout_natural_eleven` | Roll 11 on come-out → resolved, pass wins |
| `comeout_craps_two` | Roll 2 on come-out → resolved, pass loses |
| `comeout_craps_three` | Roll 3 on come-out → resolved, pass loses |
| `comeout_craps_twelve` | Roll 12 on come-out → resolved, pass loses |
| `point_made_eight` | ComeOut(8) → Point → Roll 8 → resolved, pass wins |
| `seven_out` | ComeOut(6) → Point → Roll 7 → resolved, pass loses |
| `point_phase_no_decision` | ComeOut(4) → Point → Roll 9 → continue |
| `pass_line_wins_on_natural` | PassLine $100 + natural 7 → Win($100) |
| `dont_pass_wins_on_craps_two` | DontPass $50 + craps 2 → Win($50) |
| `dont_pass_pushes_on_twelve` | DontPass + craps 12 → Push |

**Financial** — `craps_financial.rs` (14 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `verify_craps_legos` | All craps types pass `kani_proof()` |
| `debit_single_bet_correct` | debit(bankroll, bet).balance = bankroll − bet |
| `debit_rejects_over_bankroll` | debit fails when bet > bankroll |
| `debit_rejects_zero_bet` | debit(0) fails |
| `settle_win_returns_correct_balance` | Win settlement returns correct total |
| `settle_loss_reduces_balance` | Loss settlement reduces balance correctly |
| `settle_push_returns_wager` | Push returns original wager |
| `pass_line_win_payout_correct` | PassLine win = even money |
| `place_six_win_payout_correct` | Place 6 $60 → Win($70) at 7:6 |
| `place_five_win_payout_correct` | Place 5 $50 → Win($70) at 7:5 |
| `win_payout_never_zero` | ∀ win, payout > 0 |
| `lesson_level_bounded` | level ∈ [1, MAX_LEVEL] |
| `at_level_clamps` | at_level(n).level() ∈ [1, MAX_LEVEL] |
| `lesson_advancement_monotonic` | try_advance() never decreases level |

### ⭕ Tic-Tac-Toe (13 harnesses)

**Game Invariants** — `game_invariants.rs` (12 harnesses)

| Harness | Verifies |
| --------- | ---------- |
| `get_set_roundtrip` | set then get returns same value |
| `new_board_is_empty` | New board has all empty squares |
| `no_winner_on_empty_board` | Empty board has no winner |
| `opponent_returns_other_player` | opponent(X) = O and vice versa |
| `player_opponent_is_involutive` | opponent(opponent(p)) = p |
| `position_roundtrip` | Position → index → Position round-trips |
| `position_to_index_is_always_valid` | Position index ∈ [0, 8] |
| `set_marks_occupied` | Setting a square marks it occupied |
| `square_equality` | Square equality comparison correct |
| `winner_detects_column` | Column win detected correctly |
| `winner_detects_diagonal` | Diagonal win detected correctly |
| `winner_detects_row` | Row win detected correctly |

**Compositional** — `compositional_proof.rs` (1 harness)

| Harness | Verifies |
| --------- | ---------- |
| `verify_tictactoe_compositional` | All tic-tac-toe types pass `kani_proof()` |

## Verification Trifecta Status

In addition to Kani model checking, the project maintains proof sketches in
Creusot (deductive) and Verus (SMT) for cross-verifier confidence.

| Verifier | Approach | Proofs | Status |
| ---------- | ---------- | -------- | -------- |
| Kani | Model Checking | 111 | ✅ Compiles |
| Creusot | Deductive | ~10 | ✅ Compiles |
| Verus | SMT Specs | ~9 | ✅ Compiles |

## Just Recipes

| Recipe | Description | Harnesses |
| -------- | ------------- | ----------- |
| `just verify-craps` | All craps harnesses | 42 |
| `just verify-compositional` | Cross-game compositional proofs | 4 |
| `just verify-invariants` | Core game invariants | 2 |
| `just verify-financial` | Bankroll settlement proofs | 14 |
| `just verify-kani-tracked` | All tracked harnesses | 111 |
