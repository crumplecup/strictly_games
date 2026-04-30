# UI Frontend Architecture — AccessKit IR + Parallel Frontends

> **Status: complete.** All three frontends ship in `strictly_server`.
> This document describes the completed architecture.

## Overview

Every game state in Strictly Games passes through a single WCAG-verified
[AccessKit](https://accesskit.dev) IR before reaching any frontend.  The
same `VerifiedTree` is consumed by three parallel renderers:

| Frontend | Module | Transport |
|---|---|---|
| ratatui | `tui/` | Terminal (`crossterm`) |
| egui | `egui_frontend.rs` | Native window (`winit` + `wgpu`) |
| leptos | `leptos_frontend.rs` | HTTP (`axum`, HTML) |

The proof chain is **identical** regardless of frontend:

```text
GameState (*State variant)
  └→ *_to_verified_tree()            ← WcagVerified credential anchored here
       └→ VerifiedTree (AccessKit IR)
            ├── RatatuiBackend::render()  → TuiNode → render_node() → Terminal
            ├── EguiBackend::render()     → egui closures → wgpu surface
            └── LeptosRenderer::html()   → HTML+CSS → axum response body

Each path produces:
  Established<RenderComplete> → Established<*UiConsistent>
```

## Contract Alphabet

| Proposition | Meaning |
|---|---|
| `WcagVerified` | `VerifiedTree::from_parts` structural guarantee — accessible IR produced |
| `RenderComplete` | A backend rendered the IR without error |
| `TttUiConsistent` | TTT game state is faithfully represented in the frontend |
| `BjUiConsistent` | Blackjack game state is faithfully represented in the frontend |
| `CrapsUiConsistent` | Craps game state is faithfully represented in the frontend |

`*UiConsistent` implements `ProvableFrom<Established<RenderComplete>>`, so the compiler
enforces that a complete IR-to-frontend render happened before you can name the token.

## Screen Breakpoint Truth Tables

Layout is verified across industry terminal sizes (used in Kani harnesses):

| Breakpoint | Cols × Rows | Status |
|---|---|---|
| Minimum | 80 × 24 | Must pass (standard VT100) |
| Small | 100 × 30 | Must pass |
| Medium | 120 × 40 | Must pass |
| Large | 160 × 50 | Must pass |
| Ultrawide | 200 × 60 | Must pass |
| Tiny | 60 × 20 | Graceful degrade (advisory) |
| Micro | 40 × 12 | Expected failure (documented) |

`truncation_always_satisfies_label_contained` proves universally — for all
possible `(label_width, cell_width)` pairs — that truncation produces a label
that fits in its cell.

## Key Design Decisions

**AccessKit IR as the single source of truth**: `game_ir.rs` contains
`ttt_to_verified_tree`, `bj_to_verified_tree`, `craps_to_verified_tree`.  All
three frontends call these same builders; no frontend has its own IR
construction logic.

**Proof tokens are frontend-agnostic**: `TttUiConsistent` does not know whether
the game was rendered to a terminal, a window, or an HTTP response.  The token
names a consistency invariant about the game state representation, not about any
particular output format.

**`TypestateGraphWidget`**: Compositional ratatui widget — nodes as
`Block+Paragraph`, arrows as Unicode connectors (`│`, `▼`, `───▶`), callout as
a bordered block — all expressible as `TuiNode` for AccessKit bridge.

**egui frontend (`egui_frontend.rs`)**: Uses `winit 0.30` `ApplicationHandler`
pattern (not `eframe`).  `resumed()` creates the window and initialises the
`wgpu` surface; `RedrawRequested` runs the egui frame and submits to GPU.
Catppuccin Mocha theme applied via `ctx.set_visuals`.

**leptos frontend (`leptos_frontend.rs`)**: Axum HTTP server.  Each request
pulls the current game state, calls `*_to_verified_tree`, renders with
`LeptosRenderer::html()`, and returns a full HTML page.  `LeptosAppState` holds
`Arc<Mutex<Option<*State>>>` for each game.

## Five-Layer Typestate Story

| Layer | Pattern | Mechanism |
|---|---|---|
| Game logic | `GameBetting → PlayerTurn → DealerTurn → Finished` | Proof-carrying contracts (`Established<BetPlaced>`, etc.) |
| Randomness | `Shoe` (stateful) / `Dice` (stateless) generators | `elicitation::Generator` trait |
| Player decisions | Type-safe elicitation with custom styles | `ElicitCommunicator` + `Style` system |
| IR construction | `GameState → VerifiedTree` | `*_to_verified_tree()` + `WcagVerified` anchor |
| UI rendering | `VerifiedTree → {terminal, window, HTML}` | `ProvableFrom<RenderComplete>` for `*UiConsistent` |