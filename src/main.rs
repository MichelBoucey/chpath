mod cli;
mod config_file;
mod pathlist;
mod shell;

use std::env;
use std::path::Path;
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
        println!(
            "chpath {} released under 3-Clause BSD License",
            env!("CHPATH_VERSION")
        );
        println!("Copyright © 2026 Michel Boucey (michel.boucey@gmail.com)");
        return Ok(());
    }

    check_flags(&cli)?;

    let config_path = shell::config_file()?;
    let config = ConfigFile::read(&config_path)?;
    let entries = pathlist::parse(config.value());
    let home = shell::home_dir()?.to_string_lossy().into_owned();

    if cli.list {
        return list(&entries, &home);
    }

    if cli.check {
        return check(&entries, &home);
    }

    let (updated, messages) = apply(&cli, &entries, &home, &config_path)?;
    let contents = config.rewrite(&updated);

    for message in &messages {
        eprintln!("{message}");
    }

    if cli.write {
        config_file::write(&config_path, &contents, cli.backup)
    } else {
        config_file::print(&contents)
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

    if cli.force && cli.add.is_none() {
        return Err("the flag --force must be combined with the flag --add".to_string());
    }

    if cli.backup && !cli.write {
        return Err("the flag --backup must be combined with the flag --write".to_string());
    }

    if !operation {
        return Err("no operation given, run 'chpath --help'".to_string());
    }

    Ok(())
}

/// Output all the current paths of PATH, one per line, expanding the
/// pseudo-entry `$PATH` with the live value of PATH and dropping the duplicates.
fn list(entries: &[String], home: &str) -> Result<(), String> {
    let live_path = env::var("PATH").unwrap_or_default();

    for entry in pathlist::visible_paths(entries, &live_path, home) {
        println!("{entry}");
    }

    Ok(())
}

/// Output the issues detected in PATH, exactly the same lines as `--cleanup`.
fn check(entries: &[String], home: &str) -> Result<(), String> {
    print_issues(&pathlist::issues(entries, home));

    Ok(())
}

/// Output the issues detected in PATH, one per line, or `PATH is OK` when
/// nothing is found. Both `--check` and `--cleanup` use this function so that
/// they output exactly the same lines.
fn print_issues(issues: &[pathlist::Issue]) {
    if issues.is_empty() {
        println!("PATH is OK");

        return;
    }

    for issue in issues {
        println!("{}", issue.describe());
    }
}

/// Rewrite the PATH value with the given operation.
///
/// Returns the updated entries and the confirmation messages emitted by chpath
/// to stderr.
fn apply(
    cli: &Cli,
    entries: &[String],
    home: &str,
    config: &Path,
) -> Result<(Vec<String>, Vec<String>), String> {
    if let Some(path) = &cli.add {
        let (updated, added) = pathlist::add(entries, path, home);

        if added {
            pathlist::validate_add(path, home, cli.force)?;
        }

        let message = if added {
            format!("the path '{path}' has been added to PATH")
        } else {
            format!("the path '{path}' is already in PATH, PATH is left unchanged")
        };

        return Ok((updated, vec![message]));
    }

    if let Some(path) = &cli.remove {
        let updated =
            pathlist::remove(entries, path, home).map_err(|err| err.describe(path, config))?;

        return Ok((
            updated,
            vec![format!("the path '{path}' has been removed from PATH")],
        ));
    }

    if cli.cleanup {
        let cleanup = pathlist::cleanup(entries, home);
        print_issues(&cleanup.issues());

        return Ok((cleanup.kept, Vec::new()));
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
