# Verified TUI Rewrite — elicit_ui + elicit_ratatui Showcase

## Problem

Our TUI uses ~3500 lines of imperative ratatui code with direct buffer
manipulation for custom widgets. This bypasses the elicitation UI framework
entirely. Since this is an elicitation showcase project, we should rewrite
the TUI to demonstrate verified typestate UI patterns: declarative TuiNode
composition, AccessKit-based verification, compile-time screen breakpoint
truth tables, and the three-path equivalence (MCP tool → runtime render →
code emission).

## Approach

Replace imperative ratatui rendering with declarative `TuiNode` trees
verified through `Layout<Pending> → Layout<Verified> → Layout<Rendered>`.
Select four letters from elicit_ui's contract alphabet: `{HasLabel,
ValidRole, KeyboardAccessible, NoOverflow}`. Define compile-time truth
tables over industry terminal breakpoints. Refactor custom widgets into
standard widget compositions.

Work spans two repositories: upstream contributions to elicitation
(elicit_ratatui AccessKit bridge + terminal constraints) and downstream
changes in strictly_games (TUI rewrite).

### Architecture

```text
Game Typestate Machine (GameBetting → GamePlayerTurn → ...)
        ↓ (each phase produces)
TuiNode Tree (declarative layout of standard widgets)
        ↓ (From impl)
AccessKit TreeUpdate (canonical IR)
        ↓ (constraint verification)
Layout<Pending> → Layout<Verified> (HasLabel ∧ ValidRole ∧ NoOverflow ∧ KeyboardAccessible)
        ↓ (render)
Layout<Rendered> via terminal_draw()
```

### Contract Alphabet Selection

| Proposition | Why | Terminal Interpretation |
|---|---|---|
| HasLabel | Every interactive element is labeled | Prompt panes, action selectors have descriptive text |
| ValidRole | Semantic roles for all elements | Story=Log, Input=TextInput, Hands=Status, Graph=Navigation |
| KeyboardAccessible | All interaction is keyboard-driven | Trivially true for TUI, but documents the invariant |
| NoOverflow | Layout fits terminal viewport | Verified per breakpoint; prevents truncation on small terminals |

Composed: `type TerminalAccessible<T> = And<And<HasLabel<T>, ValidRole<T>>, And<KeyboardAccessible<T>, NoOverflow<T>>>`

### Screen Breakpoint Truth Tables

Industry terminal sizes verified at compile time:

| Breakpoint | Cols × Rows | Status |
|---|---|---|
| Minimum | 80 × 24 | Must pass (standard VT100) |
| Small | 100 × 30 | Must pass |
| Medium | 120 × 40 | Must pass |
| Large | 160 × 50 | Must pass |
| Ultrawide | 200 × 60 | Must pass |
| Tiny | 60 × 20 | Graceful degrade (advisory) |
| Micro | 40 × 12 | Expected failure (documented) |

Verification runs at compile time via const evaluation or Kani harnesses.
Runtime only looks up the precomputed result for the current terminal size.

### Key Design Decisions

**Compositional TypestateGraphWidget**: Redesign from cell-by-cell buffer
manipulation to a vertical stack of Block+Paragraph compositions. Nodes
become bordered Blocks with centered labels. Active node = distinct style
(Cyan border, bold label). Arrows become Paragraph lines of Unicode
connectors (│, ▼, ───▶). Callout becomes a bordered Block with Paragraph
content. Story log = Paragraph with styled Lines (already uses this).

**Compositional ChatWidget**: Refactor from buf.set_string() to
Paragraph with pre-built Line/Span objects. Left-aligned host messages,
right-aligned player messages — all expressible as styled Lines.

**AccessKit bridge in elicit_ratatui**: TuiNode → TreeUpdate From impl.
WidgetJson variants map to AccessKit Roles (Paragraph→StaticText,
List→List, Block→Group, Table→Table, etc.). Cell-based bounds map to
AccessKit bounds (1 cell = 1 unit, or scaled via configurable factor).

**VerifiedWorkflow integration**: Each game phase's render function is a
Tool with Pre/Post contracts. Pre: game is in correct phase. Post:
And<GamePhaseValid, UIVerified>. Proof tokens flow from game logic through
UI rendering.

## Todos

### Phase 1: Upstream — elicit_ratatui AccessKit Bridge

1. **accesskit-from-impls** — Add `From<TuiNode> for TreeUpdate` and
   `From<WidgetJson> for Node` implementations in elicit_ratatui. Map each
   WidgetJson variant to an AccessKit Role (Paragraph→StaticText,
   List→List, Block→Group, Table→Table, Gauge→ProgressIndicator, etc.).
   Generate NodeIds deterministically from tree position. Map cell-based
   layout constraints to AccessKit bounds.

2. **terminal-constraints** — Add terminal-specific constraint
   implementations in elicit_ratatui (or elicit_ui with a feature gate):
   MinReadableSize (minimum 3 rows × 10 cols per pane), TerminalNoOverflow
   (cell-based viewport check). Implement the Constraint trait, plug into
   ConstraintSet::builder().hard().

3. **breakpoint-verification** — Define BreakpointSet with industry terminal
   sizes. Implement compile-time truth table generation (const fn or Kani
   harnesses). Produce a BreakpointReport that maps each breakpoint to
   pass/fail/advisory.

### Phase 2: Upstream — Remove Dead egui Code from elicit_ui

4. **clean-egui-dead-code** — Remove the incorrectly integrated egui
   renderer from elicit_ui. The render plugin belongs in elicit_egui, not
   in the core ui crate. Clean up the egui-backend feature gate and
   renderer.rs.

### Phase 3: Downstream — Widget Refactoring

5. **refactor-chat-widget** — Rewrite ChatWidget to compose from
   Paragraph+Line+Span. Left-aligned host messages with Cyan styling,
   right-aligned human/agent messages. Replace all buf.set_string() calls
   with standard widget rendering. Must be expressible as WidgetJson.

6. **refactor-typestate-graph** — Redesign TypestateGraphWidget as a
   composition of Block+Paragraph widgets. Node boxes = Block with
   Borders::ALL and centered Paragraph label. Active node = distinct style
   (Cyan border, bold label). Arrows = Paragraph lines with Unicode
   connectors (│, ▼, ───▶). Callout = bordered Block with Paragraph
   content. Story log = Paragraph with styled Lines (already uses this).

### Phase 4: Downstream — Declarative TuiNode Rendering

7. **tictactoe-tuinode** — Rewrite render_tictactoe_frame to build a
   TuiNode tree. Three-column layout: game board (Block+Paragraph
   composition), story pane (Paragraph), typestate graph (Phase 3 widget
   composition). Return TuiNode instead of calling frame.render_widget
   directly.

8. **blackjack-tuinode** — Rewrite render_blackjack to build a TuiNode
   tree. Game content (dealer cards, player hands, bankroll), story pane,
   typestate graph, input pane. Thread Shoe state through the tree builder.

9. **craps-tuinode** — Rewrite render_craps to build a TuiNode tree.
   Phase-specific content, lesson indicator, story pane, typestate graph.

### Phase 5: Downstream — Verification Integration

10. **wire-verification** — In the game loop, pipe each frame's TuiNode
    through AccessKit conversion → Layout<Pending>.verify_custom() with our
    TerminalAccessible constraint set → terminal_draw() only on
    Layout<Verified>. On verification failure (terminal too small), render a
    fallback "resize your terminal" message.

11. **proof-token-flow** — Connect game proof tokens to UI proof tokens.
    Define TerminalAccessible as a Prop. Each render function takes
    Established<GamePhaseP> and returns
    Established<And<GamePhaseP, TerminalAccessible>>. Wire through the
    VerifiedWorkflow trait.

12. **breakpoint-kani-harnesses** — Write Kani proof harnesses that verify
    the compile-time truth tables. For each breakpoint, construct the
    TuiNode tree for each game phase, convert to AccessKit, verify
    constraints, assert expected pass/fail. These are compile-time proofs
    that our UI works at industry-standard terminal sizes.

### Phase 6: Documentation and Polish

13. **tui-showcase-docs** — Write TUI_VERIFICATION_GUIDE.md documenting the
    verified UI architecture: contract alphabet selection, breakpoint truth
    tables, AccessKit bridge, the five-layer typestate story. Include code
    examples showing the full pipeline from game state to verified render.

14. **update-existing-docs** — Update README.md, VERIFICATION_SHOWCASE.md,
    and BLACKJACK_GUIDE.md to reference the verified TUI architecture.

## Dependencies

```text
accesskit-from-impls ──→ terminal-constraints ──→ breakpoint-verification
                                                        ↓
clean-egui-dead-code (independent)              breakpoint-kani-harnesses
                                                        ↓
refactor-chat-widget ──┐                       wire-verification
refactor-typestate-graph──┤                          ↓
                          ↓                    proof-token-flow
                   tictactoe-tuinode                  ↓
                   blackjack-tuinode           tui-showcase-docs
                   craps-tuinode               update-existing-docs
                          ↓
                   wire-verification
```

## Notes

- Upstream work (Phases 1-2) goes to elicitation repo as PRs
- Downstream work (Phases 3-6) stays in strictly_games
- Phase 3 can proceed in parallel with Phase 1 since widget refactoring
  doesn't depend on AccessKit — just needs to target WidgetJson composition
- The egui cleanup (Phase 2) is independent and can happen anytime
- Phase 5 depends on both upstream (AccessKit bridge) and downstream
  (TuiNode rendering) being complete
- Consider whether breakpoint verification should use const fn, Kani, or
  both — const fn for the truth table, Kani for proving the table is correct
- WidgetJson covers 12/16 ratatui widgets (Canvas, Monthly, Logo, Mascot
  correctly excluded — can't serialize generics/traits or cosmetic-only)
- Cell geometry: 1 cell = 1 unit in AccessKit bounds, or configurable scale
  factor. NoOverflow math works identically for cells vs pixels.

## Five-Layer Typestate Story

| Layer | Pattern | Mechanism |
|---|---|---|
| Game logic | GameBetting → PlayerTurn → DealerTurn → Finished | Proof-carrying contracts (Established<BetPlaced>, etc.) |
| Randomness | Shoe (stateful) / Dice (stateless) generators | elicitation::Generator trait |
| Player decisions | Type-safe elicitation with custom styles | ElicitCommunicator + Style system |
| UI rendering | Layout<Pending> → Layout<Verified> → Layout<Rendered> | Contract alphabet (HasLabel ∧ ValidRole ∧ NoOverflow ∧ KeyboardAccessible) |
| UI composition | Tool call → method chain → binary | VerifiedWorkflow proving invariant preservation |
| Exhaustion      | Never                   | Returns None when empty    |