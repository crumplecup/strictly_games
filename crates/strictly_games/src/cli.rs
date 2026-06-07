//! Command-line interface for strictly_games.

use clap::{Parser, Subcommand, ValueEnum};

/// UI frontend to launch with the `tui` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum FrontendMode {
    /// Terminal UI using ratatui (default)
    #[default]
    Ratatui,
    /// Native desktop window using egui + wgpu
    Egui,
    /// Browser UI served over HTTP using leptos/axum
    Leptos,
}

/// Strictly Games - Type-safe game server with MCP interface
#[derive(Parser, Debug)]
#[command(name = "strictly_games")]
#[command(about = "Type-safe game server for LLM agents", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Subcommand to run
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the MCP game server (stdio mode)
    Server,

    /// Run the HTTP game server
    Http {
        /// Port to bind to
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Run the UI frontend (defaults to ratatui terminal)
    Tui {
        /// Frontend renderer: ratatui (terminal), egui (desktop window), leptos (browser/HTTP)
        #[arg(long, short, value_enum, default_value = "ratatui")]
        frontend: FrontendMode,

        /// Path to the database file (created if it doesn't exist)
        #[arg(long, default_value = "strictly_games.db")]
        db_path: String,

        /// Directory containing agent .toml config files
        #[arg(long)]
        agents_dir: Option<std::path::PathBuf>,

        /// Port for standalone game sessions or leptos HTTP server
        #[arg(long, default_value = "3000")]
        port: u16,
    },

    /// Run an MCP agent that plays games
    Agent {
        /// Path to agent configuration file
        #[arg(short, long, default_value = "agent_config.toml")]
        config: std::path::PathBuf,

        /// Server URL (HTTP mode) - if not provided, spawns server via stdio
        #[arg(long)]
        server_url: Option<String>,

        /// Override server command (space-separated, stdio mode only)
        #[arg(short, long)]
        server_command: Option<String>,

        /// Auto-trigger play_game for TicTacToe testing
        #[arg(long)]
        test_play: bool,

        /// Session ID for test mode play_game (optional, auto-generates if not provided)
        #[arg(long)]
        test_session: Option<String>,

        /// Auto-trigger blackjack_deal for Blackjack testing
        #[arg(long)]
        test_blackjack: bool,

        /// Initial bankroll for blackjack test mode (default: 1000)
        #[arg(long, default_value = "1000")]
        bankroll: u64,
    },

    /// Run formal verification (Kani, Verus, Creusot)
    Verify {
        /// Which tool to run: kani, verus, creusot, all
        #[arg(short, long, default_value = "all")]
        tool: String,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Convert SVG game assets (cards, dice) to ASCII art for the terminal frontend.
    ///
    /// Reads SVGs from --input, rasterizes each via resvg, then converts to ASCII
    /// using cascii at the given column width.  Writes a Rust source file of
    /// `pub const` raw-string literals that the server crate embeds at compile time.
    ///
    /// Run this whenever you want to regenerate at a different size, then
    /// recompile the workspace.
    GenerateAssets {
        /// Terminal column width for the ASCII art (e.g. 12 for cards, 7 for dice).
        #[arg(short, long)]
        columns: u32,

        /// Directory containing `cards/` and `dice/` SVG subdirectories.
        #[arg(long, default_value = "assets")]
        input: std::path::PathBuf,

        /// Destination Rust source file (overwritten on each run).
        #[arg(
            long,
            default_value = "crates/strictly_server/src/assets/ascii_art.rs"
        )]
        rust_out: std::path::PathBuf,
    },
}
