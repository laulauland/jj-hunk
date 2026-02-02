use anyhow::Result;
use clap::{Args, Parser, Subcommand};

mod diff;
mod spec;
mod commands;

use commands::{BinaryMode, ListFormat, ListGrouping, ListMode, ListOptions};

#[derive(Parser)]
#[command(name = "jj-hunk")]
#[command(about = "Programmatic hunk selection for jj")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List hunks in current changes
    List(ListArgs),

    /// Select hunks (called by jj --tool)
    Select {
        /// Path to "before" directory
        left: String,
        /// Path to "after" directory
        right: String,
    },

    /// Split changes with hunk selection
    Split {
        /// JSON/YAML spec string, or '-' for stdin (omit when using --spec-file)
        spec: Option<String>,
        /// Commit message
        message: Option<String>,
        /// Read spec from a file (JSON or YAML)
        #[arg(long = "spec-file", short = 'f')]
        spec_file: Option<String>,
        /// Revision to split (default: @)
        #[arg(short, long)]
        rev: Option<String>,
    },

    /// Commit selected hunks
    Commit {
        /// JSON/YAML spec string, or '-' for stdin (omit when using --spec-file)
        spec: Option<String>,
        /// Commit message
        message: Option<String>,
        /// Read spec from a file (JSON or YAML)
        #[arg(long = "spec-file", short = 'f')]
        spec_file: Option<String>,
    },

    /// Squash selected hunks into parent
    Squash {
        /// JSON/YAML spec string, or '-' for stdin (omit when using --spec-file)
        spec: Option<String>,
        /// Read spec from a file (JSON or YAML)
        #[arg(long = "spec-file", short = 'f')]
        spec_file: Option<String>,
        /// Revision to squash (default: @)
        #[arg(short, long)]
        rev: Option<String>,
    },
}

#[derive(Args)]
struct ListArgs {
    /// Revset to diff (e.g. @, @-, or a change id)
    #[arg(short, long)]
    rev: Option<String>,
    /// Include glob patterns (repeatable)
    #[arg(short = 'i', long)]
    include: Vec<String>,
    /// Exclude glob patterns (repeatable)
    #[arg(short = 'x', long)]
    exclude: Vec<String>,
    /// Group output by directory, extension, or status
    #[arg(long, value_enum, default_value_t = ListGrouping::None)]
    group: ListGrouping,
    /// Output format
    #[arg(long, value_enum, default_value_t = ListFormat::Json)]
    format: ListFormat,
    /// Binary handling
    #[arg(long, value_enum, default_value_t = BinaryMode::Mark)]
    binary: BinaryMode,
    /// Truncate file contents to N bytes before diffing
    #[arg(long)]
    max_bytes: Option<usize>,
    /// Truncate file contents to N lines before diffing
    #[arg(long)]
    max_lines: Option<usize>,
    /// Optional JSON/YAML spec to preview (inline or '-')
    #[arg(long)]
    spec: Option<String>,
    /// Read spec from a file (JSON or YAML)
    #[arg(long = "spec-file", short = 'f')]
    spec_file: Option<String>,
    /// Only list files with hunk counts
    #[arg(long, conflicts_with = "spec_template")]
    files: bool,
    /// Output a spec template instead of hunks
    #[arg(long = "spec-template", conflicts_with = "files")]
    spec_template: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List(args) => {
            let mode = if args.files {
                ListMode::Files
            } else if args.spec_template {
                ListMode::SpecTemplate
            } else {
                ListMode::Full
            };

            let options = ListOptions {
                rev: args.rev,
                include: args.include,
                exclude: args.exclude,
                group: args.group,
                format: args.format,
                mode,
                spec: args.spec,
                spec_file: args.spec_file,
                binary: args.binary,
                max_bytes: args.max_bytes,
                max_lines: args.max_lines,
            };

            commands::list(options)
        }
        Commands::Select { left, right } => commands::select(&left, &right),
        Commands::Split {
            spec,
            message,
            spec_file,
            rev,
        } => {
            let (spec, message) = normalize_spec_message(spec, message, &spec_file, "split")?;
            commands::split(spec.as_deref(), spec_file.as_deref(), &message, rev.as_deref())
        }
        Commands::Commit {
            spec,
            message,
            spec_file,
        } => {
            let (spec, message) = normalize_spec_message(spec, message, &spec_file, "commit")?;
            commands::commit(spec.as_deref(), spec_file.as_deref(), &message)
        }
        Commands::Squash { spec, spec_file, rev } => {
            let spec = normalize_spec_only(spec, &spec_file, "squash")?;
            commands::squash(spec.as_deref(), spec_file.as_deref(), rev.as_deref())
        }
    }
}

fn normalize_spec_message(
    mut spec: Option<String>,
    mut message: Option<String>,
    spec_file: &Option<String>,
    command: &str,
) -> Result<(Option<String>, String)> {
    if spec_file.is_some() && message.is_none() {
        message = spec.take();
    }

    let message = message
        .ok_or_else(|| anyhow::anyhow!("{command} requires a commit message"))?;

    if spec_file.is_some() {
        if spec.is_some() {
            anyhow::bail!("{command}: omit <spec> when using --spec-file");
        }
        return Ok((None, message));
    }

    let spec = spec
        .ok_or_else(|| anyhow::anyhow!("{command} requires a spec (or use --spec-file)"))?;
    Ok((Some(spec), message))
}

fn normalize_spec_only(
    spec: Option<String>,
    spec_file: &Option<String>,
    command: &str,
) -> Result<Option<String>> {
    if spec_file.is_some() {
        if spec.is_some() {
            anyhow::bail!("{command}: omit <spec> when using --spec-file");
        }
        return Ok(None);
    }

    let spec = spec
        .ok_or_else(|| anyhow::anyhow!("{command} requires a spec (or use --spec-file)"))?;
    Ok(Some(spec))
}
