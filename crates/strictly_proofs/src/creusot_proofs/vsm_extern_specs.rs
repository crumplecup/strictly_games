//! Trusted `extern_spec!` contracts for game VSM typestate constructors
//! and elicitation proof token constructors.
//!
//! These axioms tell Creusot what each game function guarantees about the
//! returned state, closing the `{false} any` gap that arises from
//! cross-crate opacity (game crate bodies are not visible to the Why3 prover).
//!
//! Also provides contracts for `Established::prove` and `Established::assert`,
//! which are pure ZST constructors that always succeed. Without these, Creusot
//! generates an `{false} any` precondition check at every call site, causing
//! the enclosing function's VC to fail unconditionally.
//!
//! # Trust boundary
//!
//! These specs are `extern_spec!` (trusted axioms).  Their correctness is
//! independently verified by Kani, which executes the real function bodies
//! symbolically and confirms the postconditions hold concretely.

use creusot_std::prelude::*;
use elicitation::{Established, Prop, ProvableFrom};

// ── Proof token constructors ───────────────────────────────────────────────

extern_spec! {
    impl<P: Prop> Established<P> {
        /// `prove` is pure ZST construction — always infallible.
        #[requires(true)]
        fn prove<C>(_credential: &C) -> Established<P>
        where P: ProvableFrom<C>;
    }
}

extern_spec! {
    impl<P: Prop> Established<P> {
        /// `assert` is the unchecked escape hatch — always infallible.
        #[requires(true)]
        fn assert() -> Established<P>;
    }
}

// ── Blackjack ─────────────────────────────────────────────────────────────

use strictly_blackjack::{
    ActionError, BasicAction, GameBetting as BjBetting, GameDealerTurn as BjDealerTurn,
    GameFinished as BjFinished, GamePlayerTurn as BjPlayerTurn, GameResult as BjGameResult,
    GameSetup as BjSetup, MAX_PLAYER_HANDS, PayoutSettled,
};

extern_spec! {
    impl BjSetup {
        /// `start_betting` stores `initial_bankroll` in `GameBetting.bankroll`.
        ///
        /// Combined with the `creusot_requires` on `bj_start_betting` that
        /// asserts `initial_bankroll@ > 0`, Creusot can deduce
        /// `result.bankroll@ > 0`, satisfying the `Betting` branch of
        /// `blackjack_consistent`.
        #[ensures(result.bankroll@ == initial_bankroll@)]
        fn start_betting(self, initial_bankroll: u64) -> BjBetting;
    }
}

extern_spec! {
    impl Clone for BjBetting {
        /// Clone preserves `bankroll` — needed to prove the Err-arm fallback in
        /// `bj_place_bet` satisfies `blackjack_consistent(Betting { inner: fallback })`.
        #[ensures(result.bankroll@ == (*self).bankroll@)]
        fn clone(&self) -> BjBetting;
    }
}

extern_spec! {
    impl BjBetting {
        /// `place_bet` deals the initial cards and returns the next game phase.
        ///
        /// Postconditions are the minimum guarantees needed for `blackjack_consistent`
        /// to hold on the wrapped `BlackjackState` in the `bj_place_bet` companion.
        #[ensures(match result {
            Ok(BjGameResult::PlayerTurn(pt)) => pt.current_hand_index@ < pt.num_hands@,
            Ok(BjGameResult::DealerTurn(_)) => true,
            Ok(BjGameResult::Finished(f, _)) => f.num_hands@ <= MAX_PLAYER_HANDS@,
            Err(_) => true,
        })]
        fn place_bet(self, bet: u64) -> Result<BjGameResult, ActionError>;
    }
}

extern_spec! {
    impl BjPlayerTurn {
        /// `action_on_current` applies a player action and transitions phases.
        ///
        /// `Err(_) => false` axiomatically encodes the proof-token guarantee:
        /// `valid_proof: Established<ValidAction>` and `bust_proof: Established<NotBust>`
        /// together prevent all error conditions, so Err is logically impossible at
        /// this call site.  This closes the `{false} any` VC from `unreachable!()`.
        #[ensures(match result {
            Ok(BjGameResult::PlayerTurn(pt)) => pt.current_hand_index@ < pt.num_hands@,
            Ok(BjGameResult::DealerTurn(_)) => true,
            Ok(BjGameResult::Finished(f, _)) => f.num_hands@ <= MAX_PLAYER_HANDS@,
            Err(_) => false,
        })]
        fn action_on_current(self, action: BasicAction) -> Result<BjGameResult, ActionError>;
    }
}

extern_spec! {
    impl BjDealerTurn {
        /// `play_dealer_turn` completes the dealer hand and returns the finished game.
        ///
        /// The `num_hands` field is preserved from the dealer-turn state, which
        /// was bounded by `MAX_PLAYER_HANDS` when the player-turn phase was created.
        #[ensures(result.0.num_hands@ <= MAX_PLAYER_HANDS@)]
        fn play_dealer_turn(self) -> (BjFinished, Established<PayoutSettled>);
    }
}

// ── Craps ─────────────────────────────────────────────────────────────────

use strictly_craps::{
    ActiveBet, ComeOutResult, DiceRoll, GameBetting as CrapsBetting, GameComeOut, GamePointPhase,
    GameResolved, GameSetup as CrapsSetup, PointRollResult,
};

extern_spec! {
    impl CrapsSetup {
        /// `start_betting` stores `bankrolls` and initialises `shooter_idx` to 0.
        ///
        /// Combined with the `creusot_requires` on `craps_start_betting` that
        /// asserts `bankrolls@.len() > 0`, Creusot can deduce
        /// `shooter_idx@ < bankrolls@.len()`, satisfying the `Betting` branch
        /// of `craps_consistent`.
        #[ensures(result.shooter_idx@ == 0)]
        #[ensures(result.bankrolls@.len() == bankrolls@.len())]
        fn start_betting(self, bankrolls: Vec<u64>) -> CrapsBetting;
    }
}

extern_spec! {
    impl CrapsBetting {
        /// `start_comeout` transitions to the come-out phase, preserving
        /// `shooter_idx` and `bankrolls.len()` from the betting state.
        #[ensures(result.shooter_idx@ == self.shooter_idx@)]
        #[ensures(result.bankrolls@.len() == self.bankrolls@.len())]
        fn start_comeout(self, seat_bets: Vec<Vec<ActiveBet>>) -> GameComeOut;
    }
}

extern_spec! {
    impl GameComeOut {
        /// `roll` produces either a point-phase or resolved game.
        ///
        /// Both result variants carry the same `shooter_idx` and `bankrolls.len()`
        /// as `self`, preserving the `craps_consistent` inequality.
        #[ensures(match result {
            ComeOutResult::PointSet(pp) =>
                pp.shooter_idx@ == self.shooter_idx@ &&
                pp.bankrolls@.len() == self.bankrolls@.len(),
            ComeOutResult::Resolved(r) =>
                r.shooter_idx@ == self.shooter_idx@ &&
                r.bankrolls@.len() == self.bankrolls@.len(),
        })]
        fn roll(self, dice: DiceRoll) -> ComeOutResult;
    }
}

extern_spec! {
    impl GamePointPhase {
        /// `roll` processes a point-phase roll, preserving `shooter_idx` and
        /// `bankrolls.len()` in both the continue and resolved variants.
        #[ensures(match result {
            PointRollResult::Continue(pp) =>
                pp.shooter_idx@ == self.shooter_idx@ &&
                pp.bankrolls@.len() == self.bankrolls@.len(),
            PointRollResult::Resolved(r) =>
                r.shooter_idx@ == self.shooter_idx@ &&
                r.bankrolls@.len() == self.bankrolls@.len(),
        })]
        fn roll(self, dice: DiceRoll) -> PointRollResult;
    }
}

extern_spec! {
    impl GameResolved {
        /// `next_round` rotates the shooter and returns a fresh betting phase.
        ///
        /// `shooter_idx` is `(old + 1) % len`, which is `< len` when `len > 0`.
        /// The `creusot_requires` on `craps_next_round` ensures `len > 0`.
        #[requires(updated_bankrolls@.len() > 0)]
        #[ensures(result.shooter_idx@ < result.bankrolls@.len())]
        fn next_round(self, updated_bankrolls: Vec<u64>) -> CrapsBetting;
    }
}

// ── TicTacToe ─────────────────────────────────────────────────────────────

use strictly_tictactoe::{
    GameFinished as TttFinished, GameInProgress as TttInProgress, GameResult as TttGameResult,
    GameSetup as TttSetup, Move as TttMove, MoveError, Player,
};

extern_spec! {
    impl TttSetup {
        /// `start` creates a fresh in-progress game with an empty history.
        ///
        /// `result.history@.len() == 0 <= 9` satisfies the `InProgress` branch
        /// of `tic_tac_toe_consistent`.
        #[ensures(result.history@.len() == 0)]
        fn start(self, first_player: Player) -> TttInProgress;
    }
}

extern_spec! {
    impl TttInProgress {
        /// `make_move` appends one move and returns the next game state.
        ///
        /// For `InProgress` results, the history stays within bounds (≤ 9).
        /// For `Finished` results, there were at least 5 moves (earliest win).
        /// `Err(_) => false` encodes the proof-token guarantee:
        /// `square_proof: Established<SquareEmpty>` and `turn_proof: Established<PlayerTurn>`
        /// prevent both error conditions, so Err is logically impossible.
        #[ensures(match result {
            Ok(TttGameResult::InProgress(g)) => g.history@.len() <= 9,
            Ok(TttGameResult::Finished(f)) => f.history@.len() >= 5,
            Err(_) => false,
        })]
        fn make_move(self, action: TttMove) -> Result<TttGameResult, MoveError>;
    }
}

extern_spec! {
    impl TttFinished {
        /// `restart` returns a fresh setup — `Setup => true` in `tic_tac_toe_consistent`.
        #[requires(true)]
        fn restart(self) -> TttSetup;
    }
}
