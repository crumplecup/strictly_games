# Strictly Games

## The Elicitation Framework Showcase: The Walled Garden Pattern

> Art is transforming chaos into form, and that's what games are.  Games have rules.  -- Stephen Sondheim

Strictly Games demonstrates the [Elicitation Framework](https://github.com/crumplecup/elicitation) in action—showing how to build **walled gardens** where invalid agent actions are **structurally impossible**.

## Why This Matters

**Traditional approach: Validate after the fact**

```
Prompt: "Only make legal moves in tic-tac-toe"
Reality: Agent tries position 10 or occupied square
Response: Return error, hope agent learns
```

**Elicitation approach: Make invalid moves unrepresentable**

```rust
// Agent calls play_game - enters walled garden
let position = elicit_position_filtered(peer, session_id).await?;
//              ↑ Only shows empty squares
//              Agent CANNOT express occupied square
//              Invalid move doesn't exist in action space

// Contracts verify what elicitation already enforced
let proof = validate_move(&action, &game)?;
execute_move(&action, &mut game, proof);
```

**We're not building better prompts or better validation. We're building action spaces where mistakes don't exist.**

## The Walled Garden Pattern

The key insight: **Agents and humans have different interfaces.**

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
        // THE WALLED GARDEN: Filter using elicitation 0.8.0 Filter trait
        let position = Position::elicit_valid_position(&board, peer.clone()).await?;
        
        // This validation should never fail (defensive check)
        session.make_move(&player_id, position)?;
    }
}
```

**What the agent sees:**
- "Choose position: 1. TopLeft, 2. Center, 5. BottomRight"
- Occupied squares are **not in the list**
- Agent **cannot express** an invalid move

### Humans: Validation-Based (Runtime Checking)

Humans/TUI call `make_move` directly with runtime validation:

```rust
#[tool(description = "Make a move at the specified position")]
pub async fn make_move(
    &self,
    req: MakeMoveRequest,
) -> Result<CallToolResult, McpError> {
    // Human provides position directly, validation catches errors
    session.make_move(&req.player_id, req.position)
        .map_err(|e| McpError::invalid_params(e, None))?;
    Ok(success)
}
```

**Why two interfaces?**
- Agents benefit from **structural prevention** (better UX, fewer retries)
- Humans need **direct control** (faster input, familiar interaction)

## Four Layers of Correctness

This codebase demonstrates how elicitation composes with other verification techniques:

### 1. **Type-Level Correctness** (Compile Time)

Positions are an enum - invalid positions don't exist:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Elicit)]
pub enum Position {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, Center, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}
// Position 10 doesn't compile, -1 doesn't compile
```

### 2. **Structural Correctness** (Elicitation Time)

Only valid options are presented:

```rust
// Filter to empty squares using the Filter trait
let valid = Position::valid_moves(&board);  // Uses select_with_filter internally

// Or elicit directly with board context
let position = Position::elicit_valid_position(&board, peer).await?;
```

### 3. **Contract Correctness** (Zero-Cost Proofs)

Declarative validation with proof-carrying types:

```rust
// Establish proof that preconditions hold
let proof = validate_move(&action, &game)?;
//          ↑ Returns Established<LegalMove>

// Execute with proof (zero-cost, PhantomData)
execute_move(&action, &mut game, proof);
//                                 ↑ Type enforces validation happened
```

### 4. **Typestate Correctness** (Phase Enforcement)

Game phases are distinct types:

```rust
let game: GameSetup = GameSetup::new();
let game: GameInProgress = game.start(Player::X);  // Consumes Setup
let result = game.make_move(action)?;  // Returns GameResult

// Can't call make_move on GameSetup (doesn't have the method)
// Can't call start on GameFinished (doesn't have the method)
```

## Tutorial: Building a Walled Garden Game

### Step 1: Define Type-Safe Domain Types

Start with enums that make invalid states unrepresentable:

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use elicitation::Elicit;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Elicit)]
pub enum Position {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, Center, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}
// Position 10 doesn't exist - can't be constructed

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Elicit)]
pub enum Player { X, O }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Elicit)]
pub enum Square {
    Empty,
    Occupied(Player),
}

#[derive(Debug, Clone, Serialize, Deserialize, Elicit)]
pub struct Board {
    squares: [Square; 9],
}
```

**Key insight:** Types used in MCP tools need:
- `Serialize` + `Deserialize` - For JSON over the wire
- `JsonSchema` - For MCP tool parameter schemas (on tool parameters only)
- `Elicit` - For elicitation framework integration

The `Elicit` derive doesn't provide serialization—you must add those yourself.

### Step 2: Implement Context-Aware Filtering

Use the elicitation 0.8.0 Filter trait for runtime filtering:

```rust
impl Position {
    /// Filters positions by board state - returns only empty squares.
    ///
    /// Uses the elicitation Filter trait to provide dynamic, context-aware
    /// selection based on runtime board state.
    pub fn valid_moves(board: &Board) -> Vec<Position> {
        Position::select_with_filter(|pos| board.is_empty(*pos))
    }

    /// Elicit a position from filtered valid moves.
    ///
    /// This method combines filtering with elicitation, using the framework's
    /// Filter trait to present only valid (empty) positions to the user.
    pub async fn elicit_valid_position(
        board: &Board,
        peer: Peer<RoleServer>,
    ) -> Result<Position, ElicitError> {
        let valid_positions = Self::valid_moves(board);
        
        if valid_positions.is_empty() {
            return Err(ElicitError::parse("No valid moves available"));
        }
        
        // Build prompt with filtered options
        let mut prompt = String::from("Please select a Position:\n\nOptions:\n");
        for (idx, pos) in valid_positions.iter().enumerate() {
            prompt.push_str(&format!("{}. {}\n", idx + 1, pos.label()));
        }
        prompt.push_str(&format!("\nRespond with number (1-{}) or label:", valid_positions.len()));
        
        // Use framework's ElicitServer
        let server = ElicitServer::new(peer);
        let response: String = server.send_prompt(&prompt).await?;
        
        // Parse response (number or label)
        let selected = if let Ok(num) = response.trim().parse::<usize>() {
            valid_positions.get(num - 1)
                .copied()
                .ok_or_else(|| ElicitError::parse("Invalid number"))?
        } else {
            Self::from_label_or_number(response.trim())
                .filter(|pos| valid_positions.contains(pos))
                .ok_or_else(|| ElicitError::parse("Invalid position"))?
        };
        
        Ok(selected)
    }
}
```

**Key changes in elicitation 0.8.0:**
- `Select::options()` returns `Vec<Self>` instead of `&'static [Self]`
- `Select::labels()` returns `Vec<String>` instead of `Vec<&'static str>`
- New `Filter` trait enables runtime filtering: `Type::select_with_filter(predicate)`
- No need for wrapper structs like `ValidPositions`

### Step 3: Implement Proof-Carrying Contracts

Use the elicitation framework's contract system:

```rust
use elicitation::contracts::{And, Established, Prop, both};

// Propositions (type-level statements)
pub struct SquareEmpty;
impl Prop for SquareEmpty {}

pub struct PlayerTurn;
impl Prop for PlayerTurn {}

// Composite proposition
pub type LegalMove = And<SquareEmpty, PlayerTurn>;

// Validation functions (establish proofs)
pub fn validate_square_empty(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<SquareEmpty>, MoveError> {
    if !game.board().is_empty(mov.position) {
        Err(MoveError::SquareOccupied(mov.position))
    } else {
        Ok(Established::assert())
    }
}

pub fn validate_player_turn(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<PlayerTurn>, MoveError> {
    if mov.player != game.to_move() {
        Err(MoveError::WrongPlayer(mov.player))
    } else {
        Ok(Established::assert())
    }
}

// Composite validation
pub fn validate_move(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<LegalMove>, MoveError> {
    let square_proof = validate_square_empty(mov, game)?;
    let turn_proof = validate_player_turn(mov, game)?;
    Ok(both(square_proof, turn_proof))
}

// Proof-carrying execution
pub fn execute_move(
    mov: &Move,
    game: &mut GameInProgress,
    _proof: Established<LegalMove>,  // Zero-cost, enforced at compile time
) {
    // Proof guarantees: square empty AND player's turn
    game.board.set(mov.position, Square::Occupied(mov.player));
    game.history.push(*mov);
}
```

### Step 4: Build Typestate State Machine

Encode game phases as distinct types:

```rust
/// Game in setup phase
pub struct GameSetup {
    board: Board,
}

/// Game in progress
pub struct GameInProgress {
    pub(super) board: Board,
    pub(super) history: Vec<Move>,
    pub(super) to_move: Player,
}

/// Game finished
pub struct GameFinished {
    board: Board,
    history: Vec<Move>,
    outcome: Outcome,  // NOT Option - always present
}

impl GameSetup {
    pub fn new() -> Self {
        Self { board: Board::new() }
    }
    
    /// Transition: Setup → InProgress
    pub fn start(self, first_player: Player) -> GameInProgress {
        GameInProgress {
            board: self.board,
            history: Vec::new(),
            to_move: first_player,
        }
    }
}

impl GameInProgress {
    /// Make move with proof-carrying validation
    pub fn make_move(self, action: Move) -> Result<GameResult, MoveError> {
        // Establish proof
        let proof = validate_move(&action, &self)?;
        
        // Execute with proof
        let mut game = self;
        execute_move(&action, &mut game, proof);
        
        // Check for game end
        if let Some(winner) = check_winner(&game.board) {
            Ok(GameResult::Finished(GameFinished {
                board: game.board,
                history: game.history,
                outcome: Outcome::Winner(winner),
            }))
        } else if is_full(&game.board) {
            Ok(GameResult::Finished(GameFinished {
                board: game.board,
                history: game.history,
                outcome: Outcome::Draw,
            }))
        } else {
            game.to_move = game.to_move.opponent();
            Ok(GameResult::InProgress(game))
        }
    }
}
```

### Step 5: Expose Agent-Only Walled Garden Tool

Create a tool that ONLY agents can call:

```rust
#[tool(description = "Play a game. You will be prompted for moves interactively.")]
pub async fn play_game(
    &self,
    peer: Peer<RoleServer>,
    req: PlayGameRequest,
) -> Result<CallToolResult, McpError> {
    // Game loop
    loop {
        let session = self.sessions.get_session(&req.session_id)?;
        
        if session.game.is_over() {
            return Ok(game_over_message);
        }
        
        if !session.is_players_turn(&player_id) {
            continue;  // Wait for opponent
        }
        
        // THE WALLED GARDEN: Filter using elicitation 0.8.0 Filter trait
        let board = session.game.board();
        let position = Position::elicit_valid_position(board, peer.clone()).await?;
        
        // Apply move (should never fail - defensive check)
        session.make_move(&player_id, position)?;
    }
}
```

### Step 6: Expose Human-Friendly Direct Tool (Optional)

For humans/TUI, provide direct access with runtime validation:

```rust
#[tool(description = "Make a move at the specified position")]
pub async fn make_move(
    &self,
    req: MakeMoveRequest,
) -> Result<CallToolResult, McpError> {
    let mut session = self.sessions.get_session(&req.session_id)?;
    
    // Human provides position directly, validation catches errors
    session.make_move(&req.player_id, req.position)
        .map_err(|e| McpError::invalid_params(e, None))?;
    
    Ok(success_message)
}
```

### Result: Four Layers of Correctness

1. **Type-level**: Position 10 doesn't compile
2. **Structural**: Occupied squares not in agent's action space
3. **Contract**: Zero-cost proof validation
4. **Typestate**: Invalid phase transitions don't compile

## Getting Started

### Quick Start: Standalone TUI Mode

The easiest way to try Strictly Games - just run the TUI and play against an AI agent:

```bash
cargo build --release
cargo run tui
```

**Controls:**
- **Arrow keys** - Move cursor to select position
- **Enter** - Place your move
- **r** - Restart game
- **q** - Quit

The TUI automatically:
1. Spawns HTTP game server on port 3000
2. Spawns AI agent connected to the server
3. Connects you as the human player
4. Cleans up subprocesses on exit

The game displays a 3x3 grid with a cursor. Move the cursor to an empty square and press Enter to place your mark. The AI agent automatically makes moves when it's their turn.

### Advanced: Distributed Mode

For development or multi-agent scenarios, run components separately:

```bash
# Terminal 1: HTTP server
cargo run http --port 3000

# Terminal 2: AI agent (plays via elicitation)
cargo run agent --server-url http://localhost:3000 --test-play

# Terminal 3: Human player (TUI)
cargo run tui --server-url http://localhost:3000
```

This demonstrates the architecture: TUI, server, and agents run independently and communicate via HTTP/MCP.

## Integrating with LLM Tools

### Connecting to Claude Desktop

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

**Configuration file locations:**

macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
Linux: `~/.config/Claude/claude_desktop_config.json`

After editing, restart Claude Desktop.

**Playing via Claude:**

```
You: Let's play tic-tac-toe!

Claude: I'll start a new game.
[calls start_game tool]

New game started!
 | | 
-+-+-
 | | 
-+-+-
 | | 

I'll play X in the center.
[calls make_move with position: Center]

Move accepted. Player O to move.
 | | 
-+-+-
 |X| 
-+-+-
 | | 
```

### Connecting to GitHub Copilot CLI

**Persistent configuration** - Edit `~/.copilot/mcp-config.json`:

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

Then run: `copilot`

**One-time use** - Pass config as command-line argument:

```bash
cd /path/to/strictly_games
copilot --additional-mcp-config '{"mcpServers":{"strictly-games":{"command":"'$(pwd)'/target/release/strictly_games","args":[],"env":{}}}}'
```

### VS Code Integration

Add to `.vscode/settings.json` in your workspace:

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

When connected via Claude Desktop or Copilot CLI, agents have access to:

### For Agents: `play_game`

**The walled garden tool** - Agents enter an elicitation loop where only valid moves are shown:

```
Tool: play_game
Arguments: { "session_id": "game1", "player_name": "Agent" }
```

The agent is prompted for each move with ONLY valid (empty) positions. The loop continues until the game ends.

### For Inspection: `get_board`

Returns current game state:
- Board display
- Current player
- Move count
- Game status

### For Session Management

- `register_player` - Join a session as X or O
- `start_game` - Reset board for new game
- `list_sessions` - See available games

### For Humans (TUI/Direct): `make_move`

Direct move submission with runtime validation:

```
Tool: make_move
Arguments: { 
  "session_id": "game1", 
  "player_id": "player1",
  "position": "Center"  // Position enum value
}
```

Used by the TUI—agents should use `play_game` instead.

## Development

```bash
# Build the project
cargo build

# Run with debug logging
RUST_LOG=strictly_games=debug,elicitation=debug cargo run

# Run tests
cargo test

# Run with all features (requires OpenCV, Tesseract)
cargo build --all-features

# Run justfile recipes
just check          # Compile only
just test-package   # Test this package
just check-all      # Full check (clippy, fmt, test)
```

## Contributing

We welcome contributions that demonstrate **verification-first development**:

1. **Design domain types** that make invalid states unrepresentable
2. **Add contracts** that encode rules declaratively  
3. **Compose contracts** to build complex validation
4. **Write Kani proofs** to verify correctness
5. **Expose via MCP** for agent interaction

See existing games for patterns. All code must follow the architecture principles above.

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

## Philosophy: The Walled Garden Pattern

### 1. Structural Prevention > Behavioral Training

```rust
// ❌ Traditional: Train agents not to make mistakes
Prompt: "Only select empty squares"
Reality: Agent tries occupied square, gets error, retries

// ✅ Walled Garden: Make mistakes structurally impossible
let valid = Position::valid_moves(&board);  // [TopLeft, Center]
let position = elicit_from_filtered(valid).await?;
// Agent cannot express occupied square - it's not in the action space
```

### 2. Agents and Humans Have Different Needs

**Agents benefit from structural prevention:**
- Fewer retries (better token efficiency)
- Clearer action space (better decisions)
- Self-documenting interface (what's valid is what's shown)

**Humans benefit from direct control:**
- Faster input (no prompts, no roundtrips)
- Familiar patterns (just call the function)
- Error feedback (learn from mistakes)

```rust
// Agent: Walled garden (only valid moves shown)
play_game(peer, session_id).await?;

// Human: Direct access (runtime validation)
make_move(session_id, player_id, position)?;
```

### 3. Elicitation Composes With Other Verification

The walled garden pattern doesn't replace other verification techniques—it **composes** with them:

```rust
// Layer 1: Type system (compile time)
enum Position { TopLeft, ... }  // Invalid positions don't exist

// Layer 2: Elicitation (action space filtering)
let valid = filter_to_empty_squares(&board);
let pos = elicit_from(valid).await?;  // Agent sees only valid

// Layer 3: Contracts (zero-cost proofs)
let proof = validate_move(&action, &game)?;
execute_move(&action, &mut game, proof);

// Layer 4: Typestate (phase enforcement)
let game: GameInProgress = ...;
game.make_move(action)?;  // Can't call this on GameSetup
```

Each layer catches different classes of errors at different times.

## Benefits

### For Agents

- **Structural correctness** - Invalid moves not in action space
- **Fewer retries** - Every shown option is valid
- **Token efficiency** - No wasted tokens on invalid attempts
- **Self-documenting** - Action space IS the specification

### For Developers

- **Separation of concerns** - Agent interface ≠ Human interface
- **Correctness by construction** - Invalid states unrepresentable
- **Compositional verification** - Multiple layers catching different errors
- **Refactoring confidence** - Compiler enforces invariants

### For System Design

- **Testability** - Pure functions, clear boundaries
- **Maintainability** - Type signatures are documentation
- **Flexibility** - Same domain logic, multiple interfaces
- **Observability** - Elicitation loops are visible, traceable

## Roadmap

**Phase 1: Foundation** (current)

- ✅ Walled garden pattern (elicitation-enforced via Filter trait)
- ✅ Proof-carrying contracts (zero-cost validation)
- ✅ Typestate state machines (compile-time phase safety)
- ✅ Dual interfaces (agents vs humans)

**Phase 2: Framework Integration**

- Support for nested elicitation (choose piece, then position)
- Multi-agent coordination via elicitation
- Richer filtering strategies (combined predicates)

**Phase 3: Expanded Games**

- Blackjack (probabilistic states, hidden information)
- Checkers (multi-step moves, larger state space)
- Chess (complex rules, piece-specific moves)

**Phase 4: Advanced Patterns**

- Tournament organization via elicitation
- Strategy elicitation and comparison
- Agent self-play with elicitation logging
- Formal verification of elicitation-driven systems

## Code Structure

The implementation demonstrates the walled garden pattern:

```
src/
├── games/tictactoe/
│   ├── position.rs       # Position enum with valid_moves filtering
│   ├── types.rs          # Player, Square, Board with #[derive(Elicit)]
│   ├── phases.rs         # Phase markers (zero-sized types)
│   ├── action.rs         # Move (domain event)
│   ├── contracts.rs      # Proof-carrying validation (Established<T>)
│   ├── typestate.rs      # GameSetup, GameInProgress, GameFinished
│   └── wrapper.rs        # Type-erased AnyGame for session management
│
├── server.rs             # MCP server with dual interfaces:
│                         #   - play_game (walled garden for agents)
│                         #   - make_move (direct access for humans)
│
├── session.rs            # Session management (players, game state)
├── agent_handler.rs      # Agent MCP client (calls play_game)
└── tui/                  # Human TUI (calls make_move)

examples/
├── agent_config.toml     # Agent configuration
├── agent_x_config.toml   # Agent X configuration
└── agent_o_config.toml   # Agent O configuration
```

### Navigation Guide

**For understanding elicitation:**
1. **src/server.rs** - See `play_game` (walled garden) vs `make_move` (direct)
2. **src/games/tictactoe/position.rs** - See `Position::elicit_valid_position()` using Filter trait
3. **src/main.rs** - See how agents call `play_game` in test mode

**For understanding contracts:**
1. **src/games/tictactoe/contracts.rs** - See proof-carrying validation
2. **src/games/tictactoe/typestate.rs** - See how contracts are used in `make_move`

**For understanding typestate:**
1. **src/games/tictactoe/phases.rs** - See phase markers
2. **src/games/tictactoe/typestate.rs** - See phase-specific methods

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

Built with the [Elicitation](https://github.com/crumplecup/elicitation) framework, demonstrating that type-safe agent interactions are practical and achievable today.

---

**"We're not validating agent moves after the fact. We're making invalid moves structurally impossible."**

**Key Insight:** The walled garden pattern—filter the action space BEFORE elicitation, so the agent cannot even express an invalid move. This is the Elicitation Framework in action.
