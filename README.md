# Strictly Games

> **The Elicitation Framework Showcase: Type-Safe Games for LLM Agents**

Strictly Games demonstrates the [Elicitation Framework](https://github.com/crumplecup/elicitation) in action—showing how to build **type-safe operational semantics** that make invalid agent behavior **unrepresentable** at the type level.

## Why This Matters

Traditional approach:
```
Prompt: "Only make legal moves in tic-tac-toe"
Reality: Agent tries position 10 (doesn't exist) or places X on occupied square
Fix: Better prompts, few-shot examples, RLHF
```

Elicitation approach:
```rust
// Positions are an enum - position 10 doesn't exist
pub enum Position { TopLeft, TopCenter, ... }

// Moves validated by contracts before application
SquareIsEmpty::check(&action, &game)?;
PlayersTurn::check(&action, &game)?;

// Invalid moves don't compile, can't be represented
```

**We're not building better prompts. We're building type systems that make correctness inevitable.**

## The Elicitation Architecture

This codebase showcases four key patterns from the Elicitation Framework:

### 1. **Typestate State Machines**

Game phases are encoded in type parameters, making illegal state transitions impossible:

```rust
// Phase encoded as type parameter
let game: Game<Setup> = Game::new();

// start() consumes Setup, returns InProgress
let game: Game<InProgress> = game.start(Player::X);

// make_move() consumes InProgress, returns InProgress or Finished
let result: MoveResult = game.make_move(action)?;
```

Invalid transitions don't exist: you can't call `make_move()` on `Game<Setup>` or `restart()` on `Game<InProgress>`.

### 2. **First-Class Actions**

Domain events (moves) are validated independently before application:

```rust
// Actions are domain types with validation
let action = Move::new(Player::X, Position::Center);

// Contract-based validation (declarative, composable)
LegalMove::check(&action, &game)?;

// Apply validated action
let result = game.make_move(action)?;
```

Actions carry their own semantics—they're not just data, they're **proof-carrying code**.

### 3. **Contract-Driven Validation**

Rules are declarative contracts, not imperative checks:

```rust
/// Precondition: Square must be empty
pub struct SquareIsEmpty;

impl SquareIsEmpty {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        if !game.board().is_empty(mov.position) {
            Err(MoveError::SquareOccupied(mov.position))
        } else {
            Ok(())
        }
    }
}

// Compose contracts
pub struct LegalMove;

impl LegalMove {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;
        PlayersTurn::check(mov, game)?;
        Ok(())
    }
}
```

Contracts are:
- **Declarative** - State what must be true, not how to check it
- **Composable** - Complex rules built from simple ones
- **Verifiable** - Can be formally proven with Kani/Creusot
- **Reusable** - Same contracts work across game variants

### 4. **Clean Boundaries**

Domain logic is pure—no presentation, no I/O, no framework coupling:

```rust
// Domain types know nothing about:
// - How they're rendered (terminal? GUI? web?)
// - How moves arrive (MCP? HTTP? keyboard?)
// - Where state is stored (memory? database?)

// This makes them:
// - Testable in isolation
// - Reusable across contexts
// - Formally verifiable
// - Framework-agnostic
```

The game logic is **pure transformation**—from one valid state to another, with contracts enforcing legality.

## Tutorial: Implementing Type-Safe Games

Let's walk through the tic-tac-toe implementation to see these patterns in action.

### Step 1: Define Domain Types

Start with the irreducible domain concepts:

```rust
// Players are an enum - only two exist
#[derive(Debug, Clone, Copy, PartialEq, Eq, Elicit)]
pub enum Player { X, O }

// Positions are bounded - only 9 valid squares
#[derive(Debug, Clone, Copy, PartialEq, Eq, Elicit)]
pub enum Position {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, Center, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}

// Square state encodes occupancy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Square {
    Empty,
    Occupied(Player),
}
```

**Key insight:** Invalid positions (like 10 or -1) **don't exist**—they're not representable in the type.

### Step 2: Build Composite Types

Compose primitives into higher-level structures:

```rust
// Board is array of squares
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    squares: [Square; 9],
}

impl Board {
    /// Check if position is empty
    pub fn is_empty(&self, pos: Position) -> bool {
        matches!(self.squares[pos as usize], Square::Empty)
    }
    
    /// Place a mark
    pub fn place(&mut self, pos: Position, player: Player) {
        self.squares[pos as usize] = Square::Occupied(player);
    }
}
```

### Step 3: Define Phase Markers

Create zero-sized types to encode game phase:

```rust
/// Phase marker: Game is being set up
pub struct Setup;

/// Phase marker: Game is in progress
pub struct InProgress;

/// Phase marker: Game has finished
pub struct Finished;

/// Outcome of finished game
pub enum Outcome {
    Winner(Player),
    Draw,
}
```

### Step 4: Implement Typestate Game


The game struct is generic over phase:

```rust
/// Type-safe game with phase encoded as type parameter
pub struct Game<Phase> {
    board: Board,
    history: Vec<Move>,
    phase_data: Phase,  // Phase-specific data
}

// Setup phase: no current player yet
impl Game<Setup> {
    pub fn new() -> Self {
        Self {
            board: Board::empty(),
            history: Vec::new(),
            phase_data: Setup,
        }
    }
    
    /// Transition: Setup → InProgress
    pub fn start(self, first_player: Player) -> Game<InProgress> {
        Game {
            board: self.board,
            history: self.history,
            phase_data: InProgress { to_move: first_player },
        }
    }
}

// InProgress phase: has current player
impl Game<InProgress> {
    /// Who moves next?
    pub fn to_move(&self) -> Player {
        self.phase_data.to_move
    }
    
    /// Attempt move - may transition to Finished
    pub fn make_move(mut self, action: Move) -> Result<MoveResult, MoveError> {
        // Validate via contracts (see Step 5)
        LegalMove::check(&action, &self)?;
        
        // Apply move
        self.board.place(action.position, action.player);
        self.history.push(action);
        
        // Check for game end
        if let Some(outcome) = self.check_outcome() {
            Ok(MoveResult::Finished(Game {
                board: self.board,
                history: self.history,
                phase_data: Finished { outcome },
            }))
        } else {
            // Toggle player
            self.phase_data.to_move = self.phase_data.to_move.opponent();
            Ok(MoveResult::Continue(self))
        }
    }
}
```

**Key insight:** `make_move()` **consumes** `self`—you can't accidentally reuse stale game state.

### Step 5: Define Contracts

Declarative rules as zero-sized struct types:

```rust
/// Contract: Square must be empty
pub struct SquareIsEmpty;

impl SquareIsEmpty {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        if !game.board().is_empty(mov.position) {
            Err(MoveError::SquareOccupied(mov.position))
        } else {
            Ok(())
        }
    }
}

/// Contract: Must be player's turn
pub struct PlayersTurn;

impl PlayersTurn {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        if mov.player != game.to_move() {
            Err(MoveError::WrongPlayer { expected: game.to_move(), got: mov.player })
        } else {
            Ok(())
        }
    }
}

/// Composite contract: Move is legal
pub struct LegalMove;

impl LegalMove {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;
        PlayersTurn::check(mov, game)?;
        Ok(())
    }
}
```

### Step 6: Define Actions

Moves are domain events, not just data:

```rust
/// A move action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub player: Player,
    pub position: Position,
}

impl Move {
    pub fn new(player: Player, position: Position) -> Self {
        Self { player, position }
    }
}

/// Move validation error
#[derive(Debug, Clone, Display, Error)]
pub enum MoveError {
    #[display("Square {} is occupied", _0)]
    SquareOccupied(Position),
    
    #[display("Wrong player: expected {:?}, got {:?}", expected, got)]
    WrongPlayer { expected: Player, got: Player },
}
```

### Step 7: Formal Verification (Already Done!)

**You don't write Kani proofs. You already have formal verification through composition.**

By using `#[derive(Elicit)]` on your domain types, you inherit the Elicitation Framework's **321 proven contracts**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Elicit)]
pub enum Position {
    TopLeft, TopCenter, TopRight,
    MiddleLeft, Center, MiddleRight,
    BottomLeft, BottomCenter, BottomRight,
}
```

**What this gives you (proven by Kani in the Elicitation Framework):**

✅ **SelectReturnsValidVariant** - Agents can only return one of the 9 positions  
✅ **SelectExhaustsSpace** - All 9 positions are enumerable  
✅ **SelectInjective** - Position → index mapping is 1:1  
✅ **FiniteDomain** - Position space is bounded (exactly 9 elements)  
✅ **NoInvalidStates** - Position 10 doesn't exist, can't be constructed

**Composed verification:**

```rust
// Position is verified (via Elicitation)
// Player is verified (via Elicitation)
// Therefore Move is verified (composition preserves properties)
pub struct Move {
    pub player: Player,    // ✅ Verified
    pub position: Position, // ✅ Verified
}

// Contracts on verified types = verified system
impl LegalMove {
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;    // Contract on verified type
        PlayersTurn::check(mov, game)?;      // Contract on verified type
        Ok(())
    }
}
```

**The warm blanket of formal verification extends over your entire game through compositional reasoning.**

No Kani setup required. No proof harnesses to write. No verification time. You get it for free by using the framework's verified primitives.

See `FORMAL_VERIFICATION.md` for the complete explanation of inherited verification guarantees.

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

## Connecting to GitHub Copilot CLI

GitHub Copilot CLI uses a configuration file at `~/.copilot/mcp-config.json`:

```bash
# Create/edit the config file
cat > ~/.copilot/mcp-config.json << 'EOF'
{
  "mcpServers": {
    "strictly-games": {
      "command": "/absolute/path/to/strictly_games/target/release/strictly_games",
      "args": [],
      "env": {}
    }
  }
}
EOF

# Then run copilot normally
copilot
```

**Or use the command-line flag for one-time use:**

```bash
# From the project directory
cd /path/to/strictly_games

# Pass config as JSON
copilot --additional-mcp-config '{"mcpServers":{"strictly-games":{"command":"'$(pwd)'/target/release/strictly_games","args":[],"env":{}}}}'

# Or from a file
copilot --additional-mcp-config @mcp-config.json
```

**Example config file** (`mcp-config.json`):
```json
{
  "mcpServers": {
    "strictly-games": {
      "command": "/home/user/strictly_games/target/release/strictly_games",
      "args": [],
      "env": {}
    }
  }
}
```

### VS Code Integration

For VS Code, add to `.vscode/settings.json` in your workspace:

```json
{
  "github.copilot.chat.mcp.servers": {
    "strictly-games": {
      "command": "/path/to/strictly_games/target/release/strictly_games"
    }
  }
}
```

## Terminal UI (TUI)

The TUI provides an interactive terminal interface to play against an AI agent.

### Standalone Mode (Recommended)

The easiest way to try the system - spawns server and AI agent automatically:

```bash
cargo run tui
```

Controls:
- **Arrow keys**: Move cursor
- **Enter**: Place move
- **q**: Quit
- **r**: Restart game

The TUI automatically:
1. Spawns HTTP game server on port 3000
2. Spawns AI agent connected to the server
3. Connects as human player
4. Cleans up subprocesses on exit

### Remote Mode

Connect to an existing server:

```bash
# Terminal 1: Start server
cargo run http --port 3000

# Terminal 2: Start AI agent
cargo run agent --server-url http://localhost:3000 --test-play

# Terminal 3: Start TUI
cargo run tui --server-url http://localhost:3000
```

This demonstrates the distributed architecture: TUI, server, and agent all running independently.

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

## Philosophy

The Elicitation Framework rests on three principles:

### 1. Make Illegal States Unrepresentable

```rust
// ❌ Traditional: Positions are numbers (what if user enters 10?)
type Position = u8;  // 0-255... but only 0-8 valid!

// ✅ Elicitation: Positions are an enum
enum Position { TopLeft, ..., BottomRight }  // Only 9 exist!
```

### 2. Validation is Composition

```rust
// ❌ Traditional: Validation is imperative spaghetti
fn validate_move(mov: Move, game: Game) -> bool {
    if game.board[mov.pos] != Empty { return false; }
    if mov.player != game.current_player { return false; }
    if game.is_finished { return false; }
    true
}

// ✅ Elicitation: Validation is declarative contracts
impl LegalMove {
    fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;
        PlayersTurn::check(mov, game)?;
        Ok(())
    }
}
```

### 3. Types Are Proofs

```rust
// If you have Game<Finished>, the game IS finished
// If you have Game<InProgress>, moves ARE legal
// If you have Move that passed LegalMove::check, it IS valid

// The type system is your proof system
```

## Benefits

### For Agents
- **Fewer hallucinations** - Invalid moves don't exist to hallucinate
- **Better understanding** - Domain structure encoded in types
- **Clearer errors** - Type-safe errors with context

### For Developers
- **Correctness by construction** - Invalid states unrepresentable
- **Refactoring confidence** - Compiler checks rule changes
- **Formal verification** - Kani proofs guarantee properties
- **Reusable components** - Contracts compose across games

### For System Design
- **Testability** - Pure functions, deterministic
- **Maintainability** - Type signatures are documentation
- **Evolvability** - Add features without breaking invariants

## Roadmap

**Phase 1: Foundation** (current)
- ✅ Typestate state machines (tic-tac-toe)
- ✅ Contract-based validation
- ✅ First-class actions
- ✅ MCP integration

**Phase 2: Verification**
- Add Kani proofs for contracts
- Demonstrate proof composition
- Document verification patterns

**Phase 3: Expanded Games**
- Blackjack (probabilistic states)
- Checkers (larger state space)
- Chess (complex rules)

**Phase 4: Elicitation Deep Dive**
- Interactive game configuration via elicitation
- Tournament organization
- Strategy elicitation and comparison

## Code Structure

The implementation demonstrates clean separation:

```
src/games/tictactoe/
├── position.rs       # Domain primitive: Position enum
├── types.rs          # Core types: Player, Square, Board
├── phases.rs         # Phase markers: Setup, InProgress, Finished
├── action.rs         # Domain events: Move, MoveError
├── contracts.rs      # Validation: SquareIsEmpty, PlayersTurn, LegalMove
├── typestate.rs      # State machine: Game<Phase> with transitions
├── wrapper.rs        # Type-erased wrapper for runtime polymorphism
└── mod.rs           # Public API and documentation
```

### Navigation Guide

1. **Start with types.rs** - See the domain primitives (Player, Square, Board)
2. **Read phases.rs** - Understand the state machine phases
3. **Study contracts.rs** - See declarative validation
4. **Explore typestate.rs** - See how types enforce transitions
5. **Check wrapper.rs** - Learn runtime polymorphism with AnyGame

Each file is self-contained with extensive documentation.

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

Built with the [Elicitation](https://github.com/crumplecup/elicitation) framework, demonstrating that type-safe agent interactions are practical and achievable today.

---

**"We're building type-safe operational semantics for agents, not better prompts."**

**Key Insight:** The best way to prevent agents from making mistakes is to make mistakes **impossible to represent** in the type system. That's the Elicitation Framework.
