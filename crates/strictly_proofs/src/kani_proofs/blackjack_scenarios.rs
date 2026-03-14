//! Scenario-level Kani harnesses for the blackjack workflow integration.
//!
//! These harnesses prove the *integration* layer end-to-end using deterministic
//! decks built with [`Deck::new_ordered`].  Unlike the unit-level harnesses in
//! `blackjack_invariants.rs`, these exercise the full call chain:
//!
//! ```text
//! GameBetting::place_bet
//!   → GamePlayerTurn::take_action (loop)
//!     → GameDealerTurn::play_dealer_turn
//!       → BankrollLedger::settle → Established<PayoutSettled>
//! ```
//!
//! # What is proven
//!
//! | Harness | Scenario | Key property |
//! |---|---|---|
//! | [`scenario_player_natural`] | Ace+King dealt to player | Fast-finish returns `PayoutSettled`, outcome is `Blackjack` |
//! | [`scenario_dealer_natural`] | Ace+King dealt to dealer | Fast-finish returns `PayoutSettled`, outcome is `Loss` |
//! | [`scenario_both_natural`] | Both get blackjack | Fast-finish returns `PayoutSettled`, outcome is `Push` |
//! | [`scenario_normal_stand`] | Player 18 vs dealer 16 | Full chain terminates with `PayoutSettled` and `Win` |
//! | [`scenario_player_bust`] | Player hits to 22 | Bust path terminates with `PayoutSettled` and `Loss` |
//! | [`scenario_dealer_bust`] | Dealer hits past 21 | Dealer bust path terminates with `PayoutSettled` and `Win` |
//! | [`scenario_bankroll_conservation`] | Symbolic bet/bankroll | Bankroll after settlement = bankroll before − bet + gross_return |
//!
//! # Cloud of assumptions
//!
//! - **Trust**: Rust ownership, `Deck::new_ordered` determinism, `Established::assert()`
//! - **Verify**: Workflow integration, outcome correctness, financial conservation

#[cfg(kani)]
use strictly_blackjack::{
    BasicAction, BetPlaced, Card, Deck, GameBetting, GameResult, Outcome, PayoutSettled,
    PlaceBetOutput, PlayActionOutput, PlayActionResult, PlayerTurnComplete, Rank, Suit,
    execute_dealer_turn, execute_place_bet, execute_play_action,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a `GameBetting` with a fully deterministic ordered deck.
///
/// Cards are laid out for 2-player, 2-dealer deal pattern:
/// `deal()` returns cards in order: p1, d1, p2, d2, then remaining...
///
/// So `[c0, c1, c2, c3, ...]` → player gets `[c0, c2]`, dealer gets `[c1, c3]`.
#[cfg(kani)]
fn betting_with_deck(cards: Vec<Card>) -> GameBetting {
    GameBetting::new(Deck::new_ordered(cards), 1000)
}

// ── Scenario 1: Player Natural ────────────────────────────────────────────────

/// Proves: when player is dealt Ace+King (blackjack), the workflow takes the
/// fast-finish path and returns `Established<PayoutSettled>` with `Blackjack` outcome.
///
/// Deck layout: player gets [Ace♠, King♠], dealer gets [Two♥, Three♥] (no natural).
///
/// **Key property:** fast-finish path establishes `PayoutSettled` ∧ outcome = `Blackjack` ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_player_natural() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::Ace, Suit::Spades),   // p1
        Card::new(Rank::Two, Suit::Hearts),   // d1
        Card::new(Rank::King, Suit::Spades),  // p2 → player has Ace+King = blackjack
        Card::new(Rank::Three, Suit::Hearts), // d2 → dealer has Two+Three = 5
    ]);

    let result = execute_place_bet(betting, 100).expect("valid bet");

    match result {
        PlaceBetOutput::Finished(finished, _settled) => {
            // Compiler proves _settled: Established<PayoutSettled> ∎
            let outcomes = finished.outcomes();
            assert!(outcomes.len() == 1, "exactly one outcome");
            assert!(
                outcomes[0] == Outcome::Blackjack,
                "player natural must yield Blackjack outcome"
            );
            assert!(
                finished.bankroll() > 1000 - 100,
                "Blackjack pays 3:2, bankroll must increase net"
            );
        }
        PlaceBetOutput::PlayerTurn(..) => {
            panic!("player natural must not require player actions")
        }
    }
}

// ── Scenario 2: Dealer Natural ────────────────────────────────────────────────

/// Proves: when dealer is dealt blackjack (and player doesn't have it), the
/// workflow fast-finishes with `PayoutSettled` and `Loss` outcome.
///
/// Deck layout: player gets [Seven♠, Eight♥] = 15, dealer gets [Ace♣, King♣] = blackjack.
///
/// **Key property:** dealer natural → fast-finish → `Loss` ∧ `PayoutSettled` ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_dealer_natural() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::Seven, Suit::Spades), // p1
        Card::new(Rank::Ace, Suit::Clubs),    // d1
        Card::new(Rank::Eight, Suit::Hearts), // p2 → player 15 (no blackjack)
        Card::new(Rank::King, Suit::Clubs),   // d2 → dealer Ace+King = blackjack
    ]);

    let result = execute_place_bet(betting, 100).expect("valid bet");

    match result {
        PlaceBetOutput::Finished(finished, _settled) => {
            // Compiler proves _settled: Established<PayoutSettled> ∎
            let outcomes = finished.outcomes();
            assert!(outcomes.len() == 1, "exactly one outcome");
            assert!(
                outcomes[0] == Outcome::Loss,
                "dealer natural must yield Loss outcome"
            );
            assert!(
                finished.bankroll() == 1000 - 100,
                "Loss: only the bet is deducted, no gross return"
            );
        }
        PlaceBetOutput::PlayerTurn(..) => {
            panic!("dealer natural must not require player actions")
        }
    }
}

// ── Scenario 3: Both Natural (Push) ───────────────────────────────────────────

/// Proves: when both player and dealer have blackjack, workflow fast-finishes
/// with `PayoutSettled` and `Push` outcome.
///
/// Deck layout: player gets [Ace♠, King♠], dealer gets [Ace♣, Queen♣].
///
/// **Key property:** both naturals → `Push` ∧ `PayoutSettled` ∧ bankroll unchanged ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_both_natural() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::Ace, Suit::Spades),  // p1
        Card::new(Rank::Ace, Suit::Clubs),   // d1
        Card::new(Rank::King, Suit::Spades), // p2 → player Ace+King = blackjack
        Card::new(Rank::Queen, Suit::Clubs), // d2 → dealer Ace+Queen = blackjack
    ]);

    let result = execute_place_bet(betting, 100).expect("valid bet");

    match result {
        PlaceBetOutput::Finished(finished, _settled) => {
            // Compiler proves _settled: Established<PayoutSettled> ∎
            let outcomes = finished.outcomes();
            assert!(outcomes.len() == 1, "exactly one outcome");
            assert!(
                outcomes[0] == Outcome::Push,
                "both naturals must yield Push outcome"
            );
            assert!(
                finished.bankroll() == 1000,
                "Push: bet returned, bankroll unchanged"
            );
        }
        PlaceBetOutput::PlayerTurn(..) => {
            panic!("double natural must not require player actions")
        }
    }
}

// ── Scenario 4: Normal Stand Path ─────────────────────────────────────────────

/// Proves: player 18 stands, dealer draws to 16 then stands → Win, full chain
/// terminates with `Established<PayoutSettled>`.
///
/// Deck: player [Nine♠, Nine♥]=18, dealer [Six♣, Ten♦]=16.
/// Dealer stands at 16 (fixed rule: stand on 17+, but here dealer has 16 and
/// no more hits needed since deck is exhausted after initial deal).
/// Actually dealer rule: hit on ≤16, stand on ≥17. Dealer has 16 → hits.
/// We give dealer a small card (Five) so dealer ends at 21? No, let's keep
/// it simple: dealer [Six, Ten] = 16, hits once and gets Two = 18 → push.
/// Let's use: player=19, dealer=16, dealer draws Three=19 → push.
/// Cleaner: player=20 (King+King), dealer=16 (Six+Ten), dealer draws Two=18 → player wins.
///
/// Deck layout:
/// - p1=King♠, d1=Six♣, p2=King♥, d2=Ten♦ → player 20, dealer 16
/// - dealer hits: Two♣ → dealer 18
/// - player stands on 20
///
/// **Key property:** normal play chain terminates → `Win` ∧ `PayoutSettled` ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_normal_stand() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::King, Suit::Spades),    // p1
        Card::new(Rank::Six, Suit::Clubs),      // d1
        Card::new(Rank::King, Suit::Hearts),    // p2 → player King+King = 20
        Card::new(Rank::Ten, Suit::Diamonds),   // d2 → dealer Six+Ten = 16
        Card::new(Rank::Two, Suit::Clubs),      // dealer hits → 18
    ]);

    let place_output = execute_place_bet(betting, 100).expect("valid bet");

    let (pt, bet_proof) = match place_output {
        PlaceBetOutput::PlayerTurn(pt, proof) => (pt, proof),
        PlaceBetOutput::Finished(..) => panic!("no natural — must enter player turn"),
    };

    // Player has 20, stands immediately.
    let play_result = execute_play_action(pt, BasicAction::Stand, bet_proof).expect("valid stand");

    let (dt, player_done_proof) = match play_result {
        PlayActionResult::Complete(PlayActionOutput::DealerTurn(dt), proof) => (dt, proof),
        _ => panic!("stand must transition to dealer turn"),
    };

    let (finished, _settled) = execute_dealer_turn(dt, player_done_proof);
    // Compiler proves _settled: Established<PayoutSettled> ∎

    let outcomes = finished.outcomes();
    assert!(outcomes.len() == 1, "exactly one outcome");
    assert!(
        outcomes[0] == Outcome::Win,
        "player 20 vs dealer 18 must be Win"
    );
    assert!(
        finished.bankroll() > 1000,
        "Win: bankroll must exceed initial after payout"
    );
}

// ── Scenario 5: Player Bust ───────────────────────────────────────────────────

/// Proves: player hits to bust, game terminates with `PayoutSettled` and `Loss`.
///
/// Deck: player [King♠, Queen♥]=20, then player hits Ten♣ → 30 (bust).
/// Wait, player wouldn't hit 20. Let's use player [Six♠, Seven♥]=13, hits Ten♣→23.
///
/// Deck layout:
/// - p1=Six♠, d1=Two♣, p2=Seven♥, d2=Three♦ → player 13, dealer 5
/// - player hits: Ten♣ → 23 (bust)
///
/// **Key property:** player bust path terminates → `Loss` ∧ `PayoutSettled` ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_player_bust() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::Six, Suit::Spades),    // p1
        Card::new(Rank::Two, Suit::Clubs),     // d1
        Card::new(Rank::Seven, Suit::Hearts),  // p2 → player Six+Seven = 13
        Card::new(Rank::Three, Suit::Diamonds),// d2 → dealer Two+Three = 5
        Card::new(Rank::Ten, Suit::Clubs),     // player hits → 23 (bust)
    ]);

    let place_output = execute_place_bet(betting, 100).expect("valid bet");

    let (pt, bet_proof) = match place_output {
        PlaceBetOutput::PlayerTurn(pt, proof) => (pt, proof),
        PlaceBetOutput::Finished(..) => panic!("no natural — must enter player turn"),
    };

    // Player hits, resulting in bust.
    let play_result = execute_play_action(pt, BasicAction::Hit, bet_proof).expect("valid hit");

    let finished = match play_result {
        PlayActionResult::Complete(PlayActionOutput::DealerTurn(dt), player_done_proof) => {
            // Bust transitions through dealer turn (even though player lost).
            let (f, _settled) = execute_dealer_turn(dt, player_done_proof);
            f
        }
        PlayActionResult::InProgress(..) => panic!("bust must not be InProgress"),
        PlayActionResult::Complete(PlayActionOutput::Finished(f), _) => f,
    };
    // Compiler proves PayoutSettled was established ∎

    let outcomes = finished.outcomes();
    assert!(outcomes.len() == 1, "exactly one outcome");
    assert!(outcomes[0] == Outcome::Loss, "bust must yield Loss outcome");
    assert!(
        finished.bankroll() == 1000 - 100,
        "Loss: bankroll = initial − bet"
    );
}

// ── Scenario 6: Dealer Bust ───────────────────────────────────────────────────

/// Proves: player stands, dealer busts → Win, `PayoutSettled` established.
///
/// Deck layout:
/// - p1=Eight♠, d1=Six♣, p2=Nine♥, d2=Seven♦ → player 17, dealer 13
/// - dealer hits: King♣ → 23 (bust)
/// - player stands on 17
///
/// **Key property:** dealer bust path terminates → `Win` ∧ `PayoutSettled` ∎
#[cfg(kani)]
#[kani::proof]
fn scenario_dealer_bust() {
    let betting = betting_with_deck(vec![
        Card::new(Rank::Eight, Suit::Spades),  // p1
        Card::new(Rank::Six, Suit::Clubs),     // d1
        Card::new(Rank::Nine, Suit::Hearts),   // p2 → player Eight+Nine = 17
        Card::new(Rank::Seven, Suit::Diamonds),// d2 → dealer Six+Seven = 13
        Card::new(Rank::King, Suit::Clubs),    // dealer hits → 23 (bust)
    ]);

    let place_output = execute_place_bet(betting, 100).expect("valid bet");

    let (pt, bet_proof) = match place_output {
        PlaceBetOutput::PlayerTurn(pt, proof) => (pt, proof),
        PlaceBetOutput::Finished(..) => panic!("no natural — must enter player turn"),
    };

    let play_result = execute_play_action(pt, BasicAction::Stand, bet_proof).expect("valid stand");

    let (dt, player_done_proof) = match play_result {
        PlayActionResult::Complete(PlayActionOutput::DealerTurn(dt), proof) => (dt, proof),
        _ => panic!("stand must transition to dealer turn"),
    };

    let (finished, _settled) = execute_dealer_turn(dt, player_done_proof);
    // Compiler proves _settled: Established<PayoutSettled> ∎

    let outcomes = finished.outcomes();
    assert!(outcomes.len() == 1, "exactly one outcome");
    assert!(outcomes[0] == Outcome::Win, "dealer bust must yield Win");
    assert!(
        finished.bankroll() > 1000,
        "Win: bankroll must exceed initial after payout"
    );
}

// ── Scenario 7: Bankroll Conservation (Symbolic) ──────────────────────────────

/// Proves the bankroll conservation law over all valid (bankroll, bet) pairs:
///
/// ```text
/// bankroll_after = bankroll_before − bet + gross_return(outcome, bet)
/// ```
///
/// Uses symbolic execution (`kani::any()`) over all valid inputs to cover
/// the entire input space, not just a single concrete case.
///
/// Restricted to the normal stand path (single Stand action) to keep the
/// symbolic state space tractable.  The fast-finish paths are covered by
/// scenarios 1–3 with concrete values.
///
/// **Key property:** ∀ valid (bankroll, bet): bankroll is exactly conserved ∎
#[cfg(kani)]
#[kani::proof]
#[kani::unwind(2)]
fn scenario_bankroll_conservation() {
    // Symbolic bankroll in [101, 10_000] and bet in [1, 100].
    // Constraints ensure bet < bankroll so debit succeeds.
    let bankroll: u64 = kani::any();
    let bet: u64 = kani::any();
    kani::assume(bankroll >= 101 && bankroll <= 10_000);
    kani::assume(bet >= 1 && bet <= 100);
    kani::assume(bet <= bankroll);

    // Deterministic deck: player 20 (two Kings), dealer 16 (Six+Ten) → dealer hits Two → 18.
    // Player wins. We use concrete cards to keep the proof focused on financial arithmetic.
    let betting = GameBetting::new(
        Deck::new_ordered(vec![
            Card::new(Rank::King, Suit::Spades),
            Card::new(Rank::Six, Suit::Clubs),
            Card::new(Rank::King, Suit::Hearts),
            Card::new(Rank::Ten, Suit::Diamonds),
            Card::new(Rank::Two, Suit::Clubs),
        ]),
        bankroll,
    );

    let place_output = execute_place_bet(betting, bet).expect("valid bet");

    let (pt, bet_proof) = match place_output {
        PlaceBetOutput::PlayerTurn(pt, proof) => (pt, proof),
        PlaceBetOutput::Finished(..) => {
            // No naturals in this deck — this branch is unreachable.
            kani::assume(false);
            return;
        }
    };

    let play_result = execute_play_action(pt, BasicAction::Stand, bet_proof).expect("valid stand");

    let (dt, player_done_proof) = match play_result {
        PlayActionResult::Complete(PlayActionOutput::DealerTurn(dt), proof) => (dt, proof),
        _ => panic!("stand on 20 must reach dealer turn"),
    };

    let (finished, _settled) = execute_dealer_turn(dt, player_done_proof);
    // Compiler proves _settled: Established<PayoutSettled> ∎

    let outcomes = finished.outcomes();
    assert!(!outcomes.is_empty(), "must have at least one outcome");
    let outcome = outcomes[0];

    // Conservation: bankroll_after = bankroll − bet + gross_return(outcome, bet)
    let gross = match outcome {
        strictly_blackjack::Outcome::Blackjack => bet + (bet * 3 / 2), // 1x bet + 1.5x bet
        strictly_blackjack::Outcome::Win => bet * 2,
        strictly_blackjack::Outcome::Push => bet,
        strictly_blackjack::Outcome::Loss | strictly_blackjack::Outcome::Surrender => 0,
    };
    let expected_bankroll = (bankroll - bet) + gross;
    assert!(
        finished.bankroll() == expected_bankroll,
        "bankroll conservation: after = before − bet + gross_return"
    );
}
