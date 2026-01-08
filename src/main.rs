use anyhow::Result;
use clap::{Parser, Subcommand};

mod diff;
mod spec;
mod commands;

#[derive(Parser)]
#[command(name = "jj-hunk")]
#[command(about = "Programmatic hunk selection for jj")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List hunks in current changes (JSON output)
    List,
    
    /// Select hunks (called by jj --tool)
    Select {
        /// Path to "before" directory
        left: String,
        /// Path to "after" directory  
        right: String,
    },
    
    /// Split changes with hunk selection
    Split {
        /// JSON spec for hunk selection
        spec: String,
        /// Commit message
        message: String,
    },
    
    /// Commit selected hunks
    Commit {
        /// JSON spec for hunk selection
        spec: String,
        /// Commit message
        message: String,
    },
    
    /// Squash selected hunks into parent
    Squash {
        /// JSON spec for hunk selection
        spec: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::List => commands::list(),
        Commands::Select { left, right } => commands::select(&left, &right),
        Commands::Split { spec, message } => commands::split(&spec, &message),
        Commands::Commit { spec, message } => commands::commit(&spec, &message),
        Commands::Squash { spec } => commands::squash(&spec),
    }
}
