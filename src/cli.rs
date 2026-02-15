//! Command-line interface for strictly_games.

use clap::{Parser, Subcommand};

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
    
    /// Run the terminal UI client
    Tui {
        /// Game server URL (HTTP). If not provided, runs in standalone mode.
        #[arg(long)]
        server_url: Option<String>,
        
        /// Port for standalone mode server
        #[arg(long, default_value = "3000")]
        port: u16,
        
        /// Path to agent config for standalone mode
        #[arg(long, default_value = "agent_config.toml")]
        agent_config: std::path::PathBuf,
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
        
        /// Auto-trigger play_game tool for testing
        #[arg(long)]
        test_play: bool,
        
        /// Session ID for test mode play_game (optional, auto-generates if not provided)
        #[arg(long)]
        test_session: Option<String>,
    },
}
