# Context-Aware Select Pattern

## Problem

The `Select` trait requires `&'static [Self]` options, making it impossible to filter based on runtime state (e.g., only show empty board positions).

## Current Server-Side Solution

**Pattern:** Retry loop with validation
- Elicitation provides ALL options (type safety)
- Server validates each selection (semantic correctness)
- Retry if invalid
- Demonstrates clean separation: `Elicitation ≠ Validation`

**Code:**
```rust
let position = loop {
    let candidate = self.elicit_position(peer.clone()).await?;
    if board.is_empty(candidate) {
        break candidate;
    }
    // Retry with feedback
};
```

**Pros:**
- Works today
- Clean layering
- Pedagogically valuable (shows composition)

**Cons:**
- Wastes LLM calls on invalid options
- Poor UX (agent doesn't know why selection failed)

## Proposed Framework Enhancement

### Option 1: Select with Filter Associated Type

**Add to `Select` trait:**
```rust
pub trait Select: Prompt + Sized {
    // Existing methods
    fn options() -> &'static [Self];
    fn labels() -> &'static [&'static str];
    fn from_label(label: &str) -> Option<Self>;
    
    // NEW: Associated type for filtering context
    type Filter = ();  // Default to no filtering
    
    // NEW: Filtered options method
    fn options_filtered(filter: &Self::Filter) -> Vec<Self> {
        // Default: return all options (no filtering)
        Self::options().to_vec()
    }
}
```

**Position implementation:**
```rust
impl Select for Position {
    type Filter = Board;
    
    fn options_filtered(board: &Board) -> Vec<Self> {
        Self::valid_moves(board)  // Only empty squares!
    }
    
    // ... other methods from derive
}
```

**Server wrapper:**
```rust
async fn elicit_position_filtered(
    &self,
    peer: Peer<RoleServer>,
    board: &Board,
) -> Result<Position, ElicitError> {
    // Call Position::options_filtered(board) under the hood
    // Present only valid options to LLM
    Position::elicit_with_filter(peer, board).await
}
```

**Benefits:**
- Minimal framework change (one associated type + one method)
- Backward compatible (default Filter = ())
- Types opt-in to filtering
- Server explicitly provides filter context
- No context threading through elicitation stack
- Framework stays stateless (filter is parameter, not state)

### Option 2: Blanket impl Select for Vec<T>

Even simpler - just implement Select for Vec of selectable items:

```rust
impl<T: Select + Clone> Select for Vec<T> {
    fn options() -> &'static [Self] {
        // Not needed - use instance method
        &[]
    }
    
    // Add instance-aware methods that use the Vec's contents
}

// Then extend Elicit trait:
impl<T: Select + Clone> Elicit for Vec<T> {
    async fn elicit(self, peer: Peer<RoleServer>) -> Result<T, ElicitError> {
        // Present only items in this Vec as options
        // User selects one, returns T (not Vec<T>)
    }
}
```

**Usage becomes trivial:**
```rust
let valid_positions = Position::valid_moves(board);  // Vec<Position>
let position = valid_positions.elicit(peer).await?;   // Position
```

**Benefits:**
- Works for ANY type that implements Select
- No wrapper structs needed
- Natural Rust idiom (Vec as container)
- Composes beautifully with filtering functions
- Framework provides one impl, all select types benefit

**This is the cleanest solution!**

## Architecture

Three layers with filtering:
1. **Elicitation** - Type safety + contextual filtering
2. **Contracts** - Proof-carrying validation
3. **Typestate** - Phase enforcement

Filtering moves LEFT (into elicitation) while maintaining clean separation.

## Implementation Notes

Current workaround in `src/server.rs`:
- `elicit_position_filtered()` wraps filtering logic
- Still calls base `elicit_position()` (TODO)
- Retry loop catches any that slip through
- Ready for framework enhancement when available
