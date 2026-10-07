mod cli;
mod config_file;
mod pathlist;
mod shell;

use std::env;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use cli::Cli;
use config_file::ConfigFile;

fn main() -> ExitCode {
    // Without flags or arguments, chpath outputs the help as the --help do.
    if env::args_os().len() == 1 {
        return print_help();
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("chpath: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Manage the Unix environment variable PATH ($PATH).
fn run() -> Result<(), String> {
    let cli = Cli::parse();

    // The flag --version outputs the version and exits as the --help do.
    if cli.version {
        println!("chpath {} released under 3-Clause BSD License", env!("CHPATH_VERSION"));
        println!("Copyright © 2026 Michel Boucey (michel.boucey@gmail.com)");
        return Ok(());
    }

    check_flags(&cli)?;

    let config_path = shell::config_file()?;
    let config = ConfigFile::read(&config_path)?;
    let entries = pathlist::parse(config.value());

    if cli.list {
        return list(&entries);
    }

    if cli.check {
        return check(&entries);
    }

    let home = shell::home_dir()?.to_string_lossy().into_owned();
    let (updated, messages) = apply(&cli, &entries, &home)?;
    let lines = config.rewrite(&updated);

    for message in &messages {
        eprintln!("{message}");
    }

    if cli.write {
        config_file::write(&config_path, &lines)
    } else {
        config_file::print(&lines)
    }
}

/// Tell which flags can be combined together.
fn check_flags(cli: &Cli) -> Result<(), String> {
    let operation =
        cli.add.is_some() || cli.remove.is_some() || cli.check || cli.cleanup || cli.list;

    if cli.write && cli.check {
        return Err("the flag --write cannot be combined with the flag --check".to_string());
    }

    if cli.write && cli.list {
        return Err("the flag --write cannot be combined with the flag --list".to_string());
    }

    if cli.write && !operation {
        return Err(
            "the flag --write must be combined with --add, --remove or --cleanup".to_string(),
        );
    }

    if !operation {
        return Err("no operation given, run 'chpath --help'".to_string());
    }

    Ok(())
}

/// Output the current paths of PATH, one per line, skipping the pseudo-entry
/// `$PATH`.
fn list(entries: &[String]) -> Result<(), String> {
    for entry in pathlist::visible_paths(entries) {
        println!("{entry}");
    }

    Ok(())
}

/// Output the dead directory paths present in the PATH value, one per line.
fn check(entries: &[String]) -> Result<(), String> {
    let home = shell::home_dir()?.to_string_lossy().into_owned();

    for dead in pathlist::dead_paths(entries, &home) {
        println!("{dead}");
    }

    Ok(())
}

/// Rewrite the PATH value with the given operation.
///
/// Returns the updated entries and the confirmation messages emitted by chpath
/// to stderr.
fn apply(cli: &Cli, entries: &[String], home: &str) -> Result<(Vec<String>, Vec<String>), String> {
    if let Some(path) = &cli.add {
        let (updated, added) = pathlist::add(entries, path);
        let message = if added {
            format!("the path '{path}' has been added to PATH")
        } else {
            format!("the path '{path}' is already in PATH, PATH is left unchanged")
        };

        return Ok((updated, vec![message]));
    }

    if let Some(path) = &cli.remove {
        let updated = pathlist::remove(entries, path)?;

        return Ok((
            updated,
            vec![format!("the path '{path}' has been removed from PATH")],
        ));
    }

    if cli.cleanup {
        let (updated, removed) = pathlist::cleanup(entries, home);
        let messages = if removed.is_empty() {
            vec!["no dead directory path has been found in PATH".to_string()]
        } else {
            removed.iter().map(|path| path.to_string()).collect()
        };

        return Ok((updated, messages));
    }

    Err("no operation given, run 'chpath --help'".to_string())
}

fn print_help() -> ExitCode {
    match Cli::command().print_help() {
        Ok(()) => {
            println!();
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("chpath: cannot print the help: {err}");
            ExitCode::FAILURE
        }
    }
}
