# Strictly Games

[![Kani Verified](https://img.shields.io/badge/Kani-Verified-brightgreen)](crates/strictly_proofs/)

## The Elicitation Framework Showcase: The Walled Garden Pattern

> Art is transforming chaos into form, and that's what games are.  Games have rules.  -- Stephen Sondheim

Strictly Games demonstrates the [Elicitation Framework](https://github.com/crumplecup/elicitation) in action—showing how to build **walled gardens** where invalid agent actions are **structurally impossible**.

**Formally verified** with Kani across the full game stack and TUI rendering layer.  Every claim in the code has a proof.

## Why This Matters

### Traditional approach: Validate after the fact

```text
Prompt: "Only make legal moves in tic-tac-toe"
Reality: Agent tries position 10 or occupied square
Response: Return error, hope agent learns
```

### Elicitation approach: Make invalid moves unrepresentable

```rust
// Agent calls play_game - enters walled garden
let position = Position::elicit_valid_position(&board, peer).await?;
//              ↑ Only shows empty squares
//              Agent CANNOT express occupied square
//              Invalid move doesn't exist in action space

// Contracts verify what elicitation already enforced
let proof = validate_move(&action, &game)?;
execute_move(&action, &mut game, proof);
```

**We're not building better prompts or better validation. We're building action spaces where mistakes don't exist.**

## Games

| Game | TUI | MCP Agent | Kani Proofs | Verified TUI |
| --- | :---: | :---: | :---: | :---: |
| Tic-Tac-Toe | ✅ | ✅ | ✅ | ✅ |
| Blackjack | ✅ | ✅ | ✅ | ✅ |
| Craps | ✅ | ✅ | ✅ | ✅ |

All three games run inside a terminal UI with a live **TypestateGraphWidget** — a real-time visualisation of the game's typestate machine that updates as you play.

## Five Layers of Correctness

This codebase demonstrates how elicitation composes with other verification techniques.

### 1. Type-Level Correctness (Compile Time)

Invalid states are inexpressible:

```rust
#[derive(Debug, Clone, Copy, Elicit)]
pub enum Position {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, Center, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}
// Position 10 doesn't compile.  -1 doesn't compile.
```

### 2. Structural Correctness (Elicitation Time)

Only valid options are presented to the agent:

```rust
// Filter to empty squares — occupied squares are not in the action space
let valid = Position::valid_moves(&board);
let position = Position::elicit_valid_position(&board, peer).await?;
//             ↑ Agent sees "1. TopLeft  2. Center" — never sees occupied squares
```

### 3. Contract Correctness (Zero-Cost Proofs)

Proof-carrying types — validation is structurally enforced, not just hoped for:

```rust
use elicitation::contracts::{And, Established, Prop, both};

pub type LegalMove = And<SquareEmpty, PlayerTurn>;

// Establish proof
let square_proof = validate_square_empty(mov, game)?;  // Established<SquareEmpty>
let turn_proof   = validate_player_turn(mov, game)?;   // Established<PlayerTurn>
let proof        = both(square_proof, turn_proof);     // Established<LegalMove>

// Execute with proof — type enforces validation happened
execute_move(mov, game, proof);
//                       ↑ If you remove validate_move, this doesn't compile
```

### 4. Typestate Correctness (Phase Enforcement)

Game phases are distinct types — invalid transitions don't compile:

```rust
let game: GameSetup      = GameSetup::new();
let game: GameInProgress = game.start(Player::X);  // Consumes Setup
let result               = game.make_move(action)?; // Returns GameResult

// Can't call make_move on GameSetup (no such method)
// Can't call start on GameFinished (no such method)
```

### 5. Verified UI (Render-Time Contracts)

The TUI rendering layer carries proof tokens through every draw call.  The `NoOverflow` contract is a composition of three props:

```rust
pub type NoOverflow = And<LabelContained, And<TextWrapped, AreaSufficient>>;

// Game phase proofs compose with layout proofs
pub fn render_blackjack<P: Prop>(
    frame: &mut Frame,
    state: &BlackjackState,
    proof: Established<P>,       // ← game phase proof comes in
) -> Result<Established<And<P, NoOverflow>>> {
    //  ↑ composed proof comes out: phase AND layout both verified
    let node = build_blackjack_tree(state);
    let layout_proof = verified_draw(frame, area, &node)?;
    Ok(both(proof, layout_proof))
}
```

**Kani verifies the arithmetic** behind all three layout props across seven terminal breakpoints:

| Breakpoint | Size | Status |
| --- | --- | --- |
| Micro | 40 × 12 | Expected failure (proven) |
| Tiny | 60 × 20 | Graceful degrade (proven) |
| Minimum | 80 × 24 | ✅ Must pass (proven) |
| Small | 100 × 30 | ✅ Must pass (proven) |
| Medium | 120 × 40 | ✅ Must pass (proven) |
| Large | 160 × 50 | ✅ Must pass (proven) |
| Ultrawide | 200 × 60 | ✅ Must pass (proven) |

`truncation_always_satisfies_label_contained` proves **universally** — no bounds, for all possible `(label_width, cell_width)` pairs — that truncation always produces a label that fits in its cell.

## WCAG-Verified Color Palette

The TUI uses a `GamePalette` built on WCAG contrast guidelines:

```rust
// All colors validated against WCAG AA contrast ratios at build time
let pal = GamePalette::default();
pal.title()   // contrast ≥ 7.0 (AAA)
pal.label()   // contrast ≥ 4.5 (AA)
pal.dimmed()  // contrast ≥ 3.0 (advisory)
```

Colors flow from the palette into `TuiNode` trees and are verified during the `NoOverflow` contract chain.

## The Walled Garden Pattern

### Agents: Elicitation-Enforced (Structural Correctness)

Agents ONLY call `play_game`, which uses elicitation internally:

```rust
#[tool(description = "Play a complete game. You will be prompted for moves.")]
pub async fn play_game(
    &self,
    peer: Peer<RoleServer>,
    req: PlayGameRequest,
) -> Result<CallToolResult, McpError> {
    loop {
        // THE WALLED GARDEN: Filter using the elicitation Filter trait
        let position = Position::elicit_valid_position(&board, peer.clone()).await?;

        // Defensive check — should never fail because elicitation filtered first
        session.make_move(&player_id, position)?;
    }
}
```

**What the agent sees:**

- `"Choose position: 1. TopLeft, 2. Center, 5. BottomRight"`
- Occupied squares are **not in the list**
- Agent **cannot express** an invalid move

### Humans: Validation-Based (Runtime Checking)

Humans and TUI call `make_move` directly with runtime validation:

```rust
#[tool(description = "Make a move at the specified position")]
pub async fn make_move(
    &self,
    req: MakeMoveRequest,
) -> Result<CallToolResult, McpError> {
    // Human provides position directly; validation catches errors
    session.make_move(&req.player_id, req.position)
        .map_err(|e| McpError::invalid_params(e, None))?;
    Ok(success)
}
```

**Why two interfaces?**

- Agents benefit from **structural prevention** (better UX, fewer retries, token efficiency)
- Humans need **direct control** (faster input, familiar interaction, error feedback)

## Getting Started

### Quick Start: Standalone TUI Mode

```bash
cargo build --release
cargo run tui
```

The TUI shows the full five-layer architecture at work:

- **TypestateGraphWidget** — live visualisation of the game's typestate machine
- **Chat panel** — agent interaction log with elicitation prompts
- **Story log** — AI-narrated game events
- **Prompt pane** — your current elicitation prompt

**Controls:**

- **Arrow keys** — Navigate / select betting amount
- **Enter** — Confirm selection
- **q** — Quit

### Advanced: Distributed Mode

Run components separately for development or multi-agent scenarios:

```bash
# Terminal 1: HTTP + MCP server
cargo run http --port 3000

# Terminal 2: AI agent (plays via elicitation loop)
cargo run agent --server-url http://localhost:3000 --test-play

# Terminal 3: Human player (TUI)
cargo run tui --server-url http://localhost:3000
```

## Integrating with LLM Tools

### Connecting to Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "/path/to/strictly_games/target/release/strictly_games"
    }
  }
}
```

macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
Linux: `~/.config/Claude/claude_desktop_config.json`

### Connecting to GitHub Copilot CLI

Edit `~/.copilot/mcp-config.json`:

```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "/absolute/path/to/strictly_games/target/release/strictly_games",
      "args": [],
      "env": {}
    }
  }
}
```

### VS Code Integration

```json
{
  "github.copilot.chat.mcp.servers": {
    "strictly-games": {
      "command": "/path/to/strictly_games/target/release/strictly_games"
    }
  }
}
```

## Available MCP Tools

### For Agents: `play_game`

**The walled garden tool** — agents enter an elicitation loop where only valid moves are shown.

```text
Tool: play_game
Arguments: { "session_id": "game1", "player_name": "Agent" }
```

### For Inspection: `get_board`

Returns current game state: board display, current player, move count, game status.

### For Session Management

- `register_player` — Join a session
- `start_game` — Reset for a new game
- `list_sessions` — See available games

### For Humans (TUI / Direct): `make_move`

Direct move submission with runtime validation.

## Formal Verification

Run the full Kani proof suite:

```bash
just verify-all              # Everything
just verify-financial        # BankrollLedger double-deduction safety
just verify-craps            # Craps dice, payout, typestate invariants
just verify-tui-breakpoints  # NoOverflow layout arithmetic, 7 breakpoints
just verify-compositional    # Cross-game compositional proofs
```

Key harnesses:

| Harness | What is proven |
| --- | --- |
| `truncation_always_satisfies_label_contained` | ∀ (label, cell): truncate output fits in cell (universal, no bounds) |
| `node_box_width_no_u16_overflow` | Label + 4 never overflows `u16` within any standard terminal |
| `symbolic_must_pass_range_safe` | ∀ (cols, rows) ∈ [80..200]×[24..60]: all must-pass layout invariants hold |
| `verify_no_double_deduction` | Bankroll cannot be debited twice for one bet |
| `verify_bankroll_legos` | Bet/settle roundtrips are balance-neutral |
| `breakpoint_micro_expected_failure` | 40×12 provably cannot fit the full layout |
| `breakpoint_tiny_graceful_degrade` | 60×20 provably exhausts the row budget |

## Development

```bash
just check-all [package]     # clippy + fmt + test for a package
just test-package [package]  # tests only
just audit                   # supply chain security
just check-features          # verify all feature flag combinations compile
```

See [`CLAUDE.md`](CLAUDE.md) for full project conventions.

## Architecture

```text
strictly_games/
├── crates/
│   ├── strictly_blackjack/   # Blackjack domain: typestate, contracts, payouts
│   ├── strictly_craps/       # Craps domain: dice, bet types, point phase
│   ├── strictly_tictactoe/   # Tic-tac-toe domain
│   ├── strictly_server/      # MCP server + TUI
│   │   └── src/tui/
│   │       ├── contracts.rs  # NoOverflow props, verified_draw, phase tokens
│   │       ├── palette.rs    # WCAG-verified GamePalette
│   │       ├── blackjack.rs  # Declarative TuiNode renderer, proof-token-flow
│   │       ├── craps.rs      # Declarative TuiNode renderer, proof-token-flow
│   │       └── typestate_widget.rs  # TypestateGraphWidget (unicode-width aware)
│   └── strictly_proofs/      # Kani, Creusot harnesses
│       └── src/kani_proofs/
│           ├── tui_breakpoints.rs   # 11 breakpoint layout proofs
│           ├── blackjack_*.rs       # Blackjack game logic proofs
│           ├── craps_*.rs           # Craps game logic proofs
│           └── bankroll_financial.rs # Financial typestate proofs
```

## Contributing

We welcome contributions that demonstrate **verification-first development**:

1. **Design domain types** that make invalid states unrepresentable
2. **Add contracts** that encode rules declaratively
3. **Compose contracts** to build complex validation
4. **Write Kani proofs** to verify correctness
5. **Expose via MCP** for agent interaction

See existing games for patterns.  All code must follow the architecture principles in [`CLAUDE.md`](CLAUDE.md).

## Dependencies

- **[elicitation](https://github.com/crumplecup/elicitation)** — Type-safe elicitation, MCP integration, contract system, TUI rendering
- **rmcp** — Model Context Protocol implementation
- **ratatui** — Terminal UI
- **tokio** — Async runtime
- **tracing** — Structured logging
- **unicode-width** — Display-column-accurate text measurement

## Philosophy

### Structural Prevention > Behavioral Training

```text
❌ Traditional: Train agents not to make mistakes
   Prompt: "Only select empty squares"
   Reality: Agent tries occupied square, gets error, retries

✅ Walled Garden: Make mistakes structurally impossible
   Agent receives: ["TopLeft", "Center"]
   Agent cannot express: "MiddleLeft" (occupied)
```

### Contracts Are Documentation That Compiles

```rust
// This function signature IS the specification:
pub fn render_blackjack<P: Prop>(
    frame: &mut Frame,
    state: &BlackjackState,
    proof: Established<P>,
) -> Result<Established<And<P, NoOverflow>>>
//   ↑ Returns: "I have proven both P and NoOverflow"
//     If layout overflows, this returns Err — caller must handle it
```

### Verification Composes

Each layer catches a different class of error at a different time.  Together they are exhaustive:

- **Compile time** — wrong types, wrong phase, missing proof
- **Elicitation time** — invalid options filtered before agent sees them
- **Contract time** — zero-cost runtime proof-carrying
- **Formal verification** — Kani proves arithmetic properties hold for ALL inputs
