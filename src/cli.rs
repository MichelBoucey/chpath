use clap::{ArgGroup, Parser};

/// Manage the Unix environment variable PATH ($PATH).
#[derive(Debug, Parser)]
#[command(name = "chpath", about, long_about = None)]
#[command(group(
    ArgGroup::new("operation")
        .args(["add", "remove", "check", "cleanup", "list"])
        .multiple(false)
))]
pub struct Cli {
    /// Add the given path to PATH.
    #[arg(short = 'a', long)]
    pub add: Option<String>,

    /// Remove the given path from PATH.
    #[arg(short = 'r', long)]
    pub remove: Option<String>,

    /// Output the dead and duplicated paths of PATH.
    #[arg(short = 'c', long)]
    pub check: bool,

    /// Remove the dead paths and the duplicate paths from PATH.
    #[arg(short = 'u', long)]
    pub cleanup: bool,

    /// Output the current paths of PATH, one per line.
    #[arg(short = 'l', long)]
    pub list: bool,

    /// Rewrite the shell configuration file instead of printing it to stdout.
    #[arg(short = 'w', long)]
    pub write: bool,

    /// Write a backup of the shell configuration file before rewriting it.
    #[arg(short = 'b', long)]
    pub backup: bool,

    /// Output the version of chpath with the current git short hash.
    #[arg(short = 'v', long)]
    pub version: bool,
}
