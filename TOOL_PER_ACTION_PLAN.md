# Implementation Plan: Tool-per-Valid-Transition (Full Refactor)

## Principle

Every agent elicitation in the codebase is replaced with dynamically registered
MCP tools. The only callable tools at any moment *are* the valid transitions
from the current game state. Invalid states become structurally impossible at
the protocol boundary — the same guarantee the typestate system provides at the
Rust type level.

```
# Before (ChoiceSet / Select paradigm)
list_tools → [play_blackjack, make_move, ...]
elicitation: "pick one of: hit, stand, double"  ← agent interprets text

# After (tool-per-valid-transition)
list_tools → [blackjack__hit, blackjack__stand]  ← these ARE the choices
             (blackjack__double absent because not valid this hand)
```

---

## Inventory of All Current Elicitations

| Location | Mechanism | What's being chosen |
|---|---|---|
| `server.rs::elicit_bet` | `ChoiceSet` | Blackjack bet amount (presets filtered by bankroll) |
| `server.rs::elicit_blackjack_action` | `ChoiceSet` | Hit / Stand (+ Double/Split when valid) |
| `server.rs::affirm_continue_blackjack` | `bool::elicit_checked` | Continue playing another hand? |
| `server.rs::elicit_position_filtered` | `TicTacToeAction::elicit` + loop | Tictactoe square (empty squares only) |
| `tui/blackjack.rs::elicit_bet_from` | `ElicitCommunicator` | Blackjack bet (TUI path) |
| `tui/blackjack.rs::elicit_action_from` | `ElicitCommunicator` | Blackjack action (TUI path) |
| `tui/craps.rs::elicit_craps_bet` | `ElicitCommunicator` | Craps bet type + amount (TUI path) |

**Scope of this plan:** MCP tool layer only (server.rs). TUI communicator path
is a separate concern — it uses `ElicitCommunicator` trait which has its own
refactor path.

---

## Refactor Map: Before → After

### 1. Tictactoe position (simplest case — pure categorical)

**Before:** `TicTacToeAction::elicit` presents all 9 positions + explore actions,
validates in a retry loop that chosen square is empty.

**After:** Register one tool per currently-empty square. Invalid squares are
absent from `list_tools` — no retry loop needed.

```
# Board state: top-left taken, center taken
list_tools → [ttt__top_center, ttt__top_right,
              ttt__middle_left, ttt__middle_right,
              ttt__bottom_left, ttt__bottom_center, ttt__bottom_right]
```

Factory: `TttMoveFactory` with `Context = BoardState`. Static tool names, filter
by `board.is_empty(position)`.

### 2. Blackjack action (categorical, context-dependent valid set)

**Before:** `ChoiceSet::new(valid_actions)` — filters `[Hit, Stand, Double,
Split, Surrender]` based on hand state, presents list, parses response.

**After:** Register one tool per valid action for the current hand.

```
# Pair of aces, sufficient bankroll
list_tools → [blackjack__hit, blackjack__stand, blackjack__split]
# Hard 20
list_tools → [blackjack__hit, blackjack__stand]
```

Factory: `BlackjackActionFactory` with `Context = ActionContext { can_double,
can_split, can_surrender }`.

### 3. Continue/stop blackjack (binary choice)

**Before:** `bool::elicit_checked` — yes/no text prompt.

**After:** Two permanent tools, always registered while a blackjack session is
active.

```
list_tools → [blackjack__deal_again, blackjack__cash_out]
```

No factory needed — these are static tools registered at session start.

### 4. Blackjack bet amount (scalar — most complex, ContextualFactory)

**Before:** `ChoiceSet::new(valid_bets)` where `valid_bets` filters
`[1, 5, 10, 25, 50, 100, 500]` by bankroll.

**After:** `BetAmountFactory` with `Context = BetConstraints { min, max,
presets }` produces:
- `bet__place` — schema-direct fast path with `"maximum": bankroll` in JSON schema
- `bet__preset_N` — one tool per preset that fits within bankroll

```
# Bankroll = $75
list_tools → [bet__place, bet__preset_50]
# (bet__preset_100, bet__preset_200, bet__preset_500 absent)
```

### 5. Craps (follow-on, same patterns)

Craps has both categorical (bet type) and scalar (bet amount) dimensions, plus
phase-gated valid bets (e.g. Pass Line only before come-out roll). Same
`ContextualFactory` approach. Deferred to after blackjack is validated.

---

## Server Architecture: Option B — Stateful Per-Action Tools

Replace the single long-running `play_blackjack` loop with a family of
phase-aware tools. Each call advances the game by one action.

### Session state model

```rust
pub enum BlackjackPhase {
    Betting(GameBetting),
    PlayerTurn(GamePlayerTurn),
    // Resolved states return immediately — no stored phase
}

pub struct BlackjackSession {
    phase: BlackjackPhase,
    registry: DynamicToolRegistry,
}
```

Stored in `SessionManager` keyed by session ID.

### Tool lifecycle per phase

```
blackjack_deal(bankroll)
  → create BlackjackSession
  → register: bet__place, bet__preset_N... (filtered by bankroll)
  → notify_tool_list_changed
  → return: visible cards, bankroll display

[agent calls bet__preset_100 or bet__place({amount:75})]
  → place_bet → GamePlayerTurn
  → unregister bet__ tools
  → register: blackjack__hit, blackjack__stand [, __double, __split]
  → notify_tool_list_changed
  → return: player hand, dealer upcard

[agent calls blackjack__hit]
  → take_action(Hit) → new GamePlayerTurn or GameResolved
  → if still PlayerTurn: re-register with updated action context
  → if resolved: unregister all game tools
  → notify_tool_list_changed
  → return: updated hand, result if resolved

[agent calls blackjack__deal_again or blackjack__cash_out]
  → deal_again: new hand with remaining bankroll → back to betting phase
  → cash_out: destroy session, return final bankroll
```

### Server routing integration: PluginRegistry

`PluginRegistry` is the `ServerHandler`. It aggregates plugins via
`ElicitPlugin`. `DynamicToolRegistry` already implements `ElicitPlugin`.
`GameServer`'s static tools become an `ElicitPlugin` too.

**Migration:**

1. Remove `#[tool_handler(router = self.tool_router)]` from `GameServer` — it
   will no longer be the `ServerHandler` directly.

2. Implement `ElicitPlugin` on `GameServer`, delegating `list_tools` /
   `call_tool` to the existing `ToolRouter<Self>`:

```rust
impl ElicitPlugin for GameServer {
    fn name(&self) -> &'static str { "game" }

    fn list_tools(&self) -> Vec<Tool> {
        self.tool_router.list_tools()
    }

    fn call_tool<'a>(&'a self, params, ctx) -> BoxFuture<'a, ...> {
        Box::pin(self.tool_router.call_tool(params, ctx))
    }
}
```

3. Per-connection setup builds a `PluginRegistry` combining both:

```rust
// Called once per HTTP connection (stateful_mode = true means one handler per session)
fn build_handler(sessions: Arc<SessionManager>) -> PluginRegistry {
    let game_server = GameServer::with_sessions(sessions);
    let dynamic_registry = DynamicToolRegistry::new();  // per-session, starts empty

    PluginRegistry::new()
        .register_flat(game_server)       // static tools: no prefix (e.g. "make_move")
        .register("dyn", dynamic_registry) // dynamic tools: "dyn__bet__preset_100" etc.
}
```

4. `BlackjackSession` holds a reference to the per-connection
   `DynamicToolRegistry` (passed in at deal time) and calls
   `registry.register_contextual(...)` + `registry.notify_tool_list_changed()`
   on each phase transition.

The `#[tool_router]` macro and all `#[tool(...)]` annotations on `GameServer`
are **unchanged** — they continue to generate `ToolRouter<GameServer>`. Only
the outermost `ServerHandler` changes from `GameServer` to `PluginRegistry`.

---

## Implementation Phases

### Phase 1: Types + Factories (strictly_blackjack, no server changes)

1. `crates/strictly_blackjack/src/bet_amount.rs` — new file
   - `BetAmount(u64)` newtype
   - `#[cfg_attr(feature = "shuffle", derive(elicitation_derive::Rand))]`
   - `#[rand(bounded(1, 10_001))]` for simulation
   - `BetConstraints { min: u64, max: u64, presets: &'static [u64] }`
   - `BetAmountFactory` implementing `ContextualFactory<Context = BetConstraints>`

2. `crates/strictly_blackjack/src/action.rs` — extend
   - `PlayerActionContext { can_double, can_split, can_surrender }`
   - `BlackjackActionFactory` implementing `ContextualFactory<Context = PlayerActionContext>`

3. Update `GameBetting::place_bet(BetAmount)` and `BankrollLedger::debit`

### Phase 2: Session State (strictly_server)

4. `crates/strictly_server/src/games/blackjack/session.rs` — new file
   - `BlackjackPhase` enum
   - `BlackjackSession` struct with `DynamicToolRegistry`
   - Phase transition methods that re-register tools and notify peer

5. `SessionManager` — add blackjack session storage

### Phase 3: PluginRegistry Migration + Phase Tools (strictly_server)

6. `server.rs` — implement `ElicitPlugin` on `GameServer` (delegates to `tool_router`)
7. `server.rs` — remove `#[tool_handler(...)]` impl, replace with `PluginRegistry`-based server factory
8. `server.rs` — add `blackjack_deal` tool (replaces `play_blackjack`)
9. `server.rs` — add `blackjack__hit`, `blackjack__stand`, etc. as dispatchers into `BlackjackSession`
10. Remove `elicit_bet`, `elicit_blackjack_action`, `affirm_continue_blackjack`
11. Remove `ChoiceSet` / `ElicitServer` usage from blackjack path

### Phase 4: Tictactoe (strictly_server)

10. `TttMoveFactory` with `Context = BoardState`
11. Replace `elicit_position_filtered` retry loop with dynamic tool registration
12. Update `make_move` to check session registry for ttt__ tools

### Phase 5: Craps (follow-on)

Same pattern. Deferred until blackjack + tictactoe validated.

---

## Resolved

1. **Session ID**: Agent is not responsible for session bookkeeping. Since we
   summon the agent for the game, the server knows the session. Session is
   implicit per HTTP connection (`stateful_mode = true` gives each connection
   its own handler + `PluginRegistry`). Phase tools (`blackjack__hit`, etc.)
   carry no `session_id` parameter.

## Open Questions (explore as we go)

1. **`notify_tool_list_changed` peer injection**: Peer available in each
   `call_tool` context; pass into `BlackjackSession` on each call.

2. **Craps timing**: follow-on after blackjack validated.

3. **TUI communicator path**: `elicit_bet_from` / `elicit_action_from` /
   `elicit_craps_bet` use `ElicitCommunicator` — separate refactor.
