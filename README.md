# Strictly Games

> **Type-safe operational semantics for LLM agents**

Strictly Games is an MCP (Model Context Protocol) server that provides verified game environments where agents can play. The name captures our approach: **strictly typed** operational semantics that create **strict boundaries** for agent behavior.

## Vision

We're not building better prompts—we're building **type-safe operational semantics for agents**.

Traditional approaches rely on instructions like "don't make invalid moves." Strictly Games makes invalid moves **unrepresentable** at the type level. Agents interact through a well-defined protocol where:

- **Invalid states don't exist** - The type system enforces game rules
- **Operations require proofs** - Moves carry evidence of legality
- **Verification is compositional** - Complex games built from verified primitives

This is a **walled garden** approach: agents operate in environments with rigorous constraints, demonstrating that formal methods can guide AI systems through type-safe interfaces rather than prompt engineering.

## Current Games

### Tic-Tac-Toe

A 3×3 game demonstrating the core concepts:
- Type-safe board representation
- Move validation at the API boundary
- Win/draw detection
- Full game state tracking

Future games: Blackjack, Checkers, Chess, Go

## Installation

```bash
# Clone the repository
git clone https://github.com/crumplecup/strictly_games.git
cd strictly_games

# Build the server
cargo build --release
```

## Running the Server

The server communicates via stdin/stdout using the MCP protocol:

```bash
# Run directly
cargo run

# Or use the built binary
./target/release/strictly_games
```

The server will start and wait for MCP messages on stdin. You'll see:

```
Starting Strictly Games MCP server
Server ready - connect via MCP protocol
```

## Connecting to Claude Desktop

Add to your Claude Desktop MCP configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "/path/to/strictly_games/target/release/strictly_games"
    }
  }
}
```

On macOS:
```bash
# Edit config
code ~/Library/Application\ Support/Claude/claude_desktop_config.json

# Restart Claude Desktop
```

On Linux:
```bash
# Edit config
code ~/.config/Claude/claude_desktop_config.json

# Restart Claude Desktop
```

## Connecting to GitHub Copilot

Configure in VS Code's MCP settings or via CLI.

## Playing Tic-Tac-Toe

Once connected, ask Claude or Copilot to play:

```
You: Let's play tic-tac-toe!

Claude: I'll start a new game.
[calls start_game tool]

New game started!
1|2|3
-+-+-
4|5|6
-+-+-
7|8|9

I'll play X in the center.
[calls make_move with position: 4]

Move accepted. Player O to move.
1|2|3
-+-+-
4|X|6
-+-+-
7|8|9
```

### Available Tools

**`start_game`**
- Starts a new tic-tac-toe game
- Player X goes first
- Returns the empty board

**`make_move`**
- Arguments: `position` (0-8, where 0=top-left, 8=bottom-right)
- Validates the move (square must be empty, game in progress)
- Returns updated board and game status
- Example: `{"position": 4}` plays center square

**`get_board`**
- Returns current board state
- Shows current player, game status, move count

### Board Layout

Positions are numbered 0-8:

```
0|1|2
-+-+-
3|4|5
-+-+-
6|7|8
```

Displayed with numbers for empty squares, X/O for occupied:

```
X|O|3
-+-+-
X|5|O
-+-+-
7|8|9
```

## Development

```bash
# Run with debug logging
RUST_LOG=strictly_games=debug,elicitation=debug cargo run

# Run tests (once implemented)
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

## Architecture

```
src/
├── main.rs           # MCP server entry point
├── server.rs         # Tool router and handlers
└── games/
    └── tictactoe/
        ├── types.rs  # Domain types (Player, Board, GameState)
        └── rules.rs  # Game logic and validation
```

All domain types derive `Elicit` for future interactive elicitation support.

## Dependencies

- **[elicitation](https://github.com/crumplecup/elicitation)** - Type-safe elicitation and MCP integration
- **rmcp** - Model Context Protocol implementation
- **tokio** - Async runtime
- **tracing** - Structured logging

## Roadmap

**Phase 1: Foundation** (current)
- ✅ Basic MCP server infrastructure
- ✅ Tic-tac-toe with move validation
- ✅ Full observability via tracing

**Phase 2: Contracts**
- Add Kani-verified contracts for move legality
- Demonstrate proof-carrying code pattern
- Contract composition for complex operations

**Phase 3: Expanded Games**
- Blackjack (probabilistic verification)
- Checkers (game tree search)
- Chess (complex state space)

**Phase 4: Elicitation Integration**
- Interactive game configuration
- Tournament organization
- Strategy elicitation

## Philosophy

Traditional approach:
```
Prompt: "Only make legal moves"
Reality: Agents hallucinate illegal moves
Fix: Better prompts, fine-tuning, RLHF
```

Strictly Games approach:
```
API: move(pos: Position) requires proof(is_empty(pos))
Reality: Invalid moves don't compile
Fix: Not needed - prevented by types
```

We believe formal methods and type systems provide better agent guardrails than instructions in natural language.

## Contributing

This project demonstrates **verification-first development**:
1. Design domain types that make invalid states unrepresentable
2. Add contracts that encode game rules
3. Use verification tools (Kani, Creusot) to prove correctness
4. Expose verified operations through MCP

See `tictactoe.md` for the detailed design philosophy.

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

Built with the [Elicitation](https://github.com/crumplecup/elicitation) framework, demonstrating that type-safe agent interactions are practical and achievable today.

---

**"We're building type-safe operational semantics for agents, not better prompts."**
