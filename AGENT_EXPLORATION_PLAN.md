# Agent Exploration Prompts — Select + Filter Showcase

## Overview

When it's an AI agent's turn, we want them to inspect game state before
committing to a move — just as a human player glances at their hand,
checks the dealer's card, or surveys the board.  This is implemented
using elicitation's **Select** enum with **Filter**-gated variants.

## Architecture

Each game defines a single action enum containing both **commit**
variants (actual game moves) and **explore** variants (state queries).
Filter controls which options surface per participant type:

- **Agents** see the full pool (explore + commit)
- **Humans** see commit-only (they already see state in the TUI)

When an agent picks an explore variant the game builds a live state
snapshot, formats it, and sends it back through the communicator.
Elicitation then restarts.  When the agent picks a commit variant the
underlying game action is extracted and the turn proceeds.

```text
Agent's Turn
    ↓
BlackjackAction::elicit(comm)
    ↓
[Hit, Stand, ViewHand, ViewDealerCard, ViewShoeStatus, ...]
    ↓ agent picks...
  ┌─────────────────────┬────────────────────────────┐
  │ commit (Hit/Stand)  │ explore (ViewHand/etc.)     │
  │ → execute move      │ → build live state snapshot │
  │ → done              │ → send_prompt to agent      │
  │                     │ → loop back to elicit       │
  └─────────────────────┴────────────────────────────┘
```

## Per-Game Action Enums

### BlackjackAction

Located in `crates/strictly_blackjack/src/explore.rs`.

| Variant | Kind | Description |
| --- | --- | --- |
| Hit | commit | Take another card |
| Stand | commit | Keep current hand |
| ViewHand | explore | Cards and total in your hand |
| ViewDealerCard | explore | Dealer's visible up card |
| ViewOtherPlayers | explore | Other players' visible cards |
| ViewShoeStatus | explore | Cards remaining in shoe |
| ViewBankroll | explore | Current chip count |

Helper: `to_basic_action()` extracts the `BasicAction` for game processing.

### TicTacToeAction

Located in `crates/strictly_tictactoe/src/explore.rs`.

| Variant | Kind | Description |
| --- | --- | --- |
| Play(Position) | commit | Place mark at position |
| ViewBoard | explore | Current board state (3×3 grid) |
| ViewLegalMoves | explore | Available empty positions |
| ViewThreats | explore | Immediate win/block opportunities |

Helper: `to_position()` extracts the `Position` for game processing.

### CrapsAction

Located in `crates/strictly_craps/src/explore.rs`.

| Variant | Kind | Description |
| --- | --- | --- |
| PlaceBet | commit | Place a new bet (amount elicited separately) |
| Done | commit | Finished betting |
| ViewPoint | explore | Current point (if established) |
| ViewActiveBets | explore | Your bets on the table |
| ViewOtherBets | explore | Other players' visible bets |
| ViewRollHistory | explore | Recent dice results |
| ViewBankroll | explore | Current chip count |

## View Types and ElicitSpec

Each game has a view type that snapshots live state for agent inspection.
These implement `ElicitSpec` with categories mapping 1:1 to explore
variants, and register in the TypeSpec inventory.

| View Type | Location | Categories |
| --- | --- | --- |
| `BlackjackPlayerView` | `strictly_blackjack/src/view.rs` | your_hand, dealer_showing, other_players, shoe_status, bankroll |
| `TicTacToeView` | `strictly_tictactoe/src/view.rs` | board, legal_moves, threats |
| `CrapsTableView` | `strictly_craps/src/view.rs` | point, active_bets, other_bets, roll_history, bankroll |

Constructors build from live game state:

```rust
BlackjackPlayerView::from_game_state(&GamePlayerTurn, seat_index, bankroll)
BlackjackPlayerView::from_multi_round(&MultiRound, seat_idx, bankroll)
TicTacToeView::from_board(&Board, current_player)
CrapsTableView::from_betting(bankroll)
CrapsTableView::from_point_phase(point, bets, other_bets, history, bankroll)
```

## Filter Design

Each action enum provides `is_commit()` / `is_explore()` predicates:

```rust
impl BlackjackAction {
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Hit | Self::Stand)
    }
}
```

The explore loop in each game session uses these to drive the
commit-or-explore cycle.  Humans use the existing elicitation path
(only commit actions).  Agents use the new explore-aware functions.

## Explore Loop Integration

### Blackjack (TUI multi-player)

`elicit_agent_action()` in `strictly_server/src/tui/blackjack.rs`:

- Agents: `BlackjackAction::elicit(comm)` → explore loop
- Humans: `elicit_action_from(comm)` → `BasicAction::elicit()` (unchanged)
- Fork point: `SeatComm::Human` vs `SeatComm::Agent` dispatch

### Tic-Tac-Toe (MCP server)

`elicit_position_filtered()` in `strictly_server/src/server.rs`:

- Replaced the stub (`valid_positions[0]`) with full explore loop
- `TicTacToeAction::elicit(comm)` → explore or commit
- Uses `ElicitServer::new(peer)` as communicator
- Validates committed position is actually empty

### Craps (TUI multi-player)

`elicit_agent_craps_bet()` in `strictly_server/src/tui/craps.rs`:

- Agents: `CrapsAction::elicit(comm)` → explore loop → then styled `u64` bet
- Humans: `elicit_craps_bet(comm, ...)` → styled `u64` directly (unchanged)
- Fork point: `CrapsSeatComm::Human` vs `CrapsSeatComm::Agent` dispatch

## Story Pane Narration

When agents explore, human-readable events appear in the story pane:

```text
🔍 Claude checks their hand
🔍 Claude checks dealer's up card
🔍 Claude checks the shoe
🎯 Claude's turn
  🃏 Claude hits → receives 5♦, total 16
🔍 Claude checks their hand
  🃏 Claude stands
```

Each explore variant maps to a distinct narration string, making agent
reasoning visible to the human player watching the TUI.

## Personality Interaction

Exploration behavior emerges from the agent personality system with no
special code.  The personality's system prompt and the available options
naturally produce different exploration patterns:

- A **cautious** agent explores multiple categories before committing
- An **impulsive** agent goes straight to a commit action
- A **card-counter** explores shoe_status every turn
- A **conservative** agent checks bankroll before betting

## Elicitation Features Showcased

1. **Select** — static pool of commit + explore options per enum
2. **Filter** — runtime participant-aware option subsetting (is_commit)
3. **ElicitSpec / TypeSpec** — structured game state introspection
4. **Elicit derive** — automatic prompt generation for action enums
5. **send_prompt** — delivering explore results back to agent context
6. **Styled elicitation** — craps bet amount uses `with_style::<u64, _>()`
