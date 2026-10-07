use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use regex::Regex;

use crate::pathlist;

/// The shell configuration file with the line which sets the variable PATH.
pub struct ConfigFile {
    lines: Vec<String>,
    path_line: usize,
    prefix: String,
    value: String,
}

impl ConfigFile {
    /// Read the shell configuration file and find the line which sets PATH.
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|err| format!("cannot read the file {}: {err}", path.display()))?;
        let reader = BufReader::new(file);
        let lines = reader
            .lines()
            .collect::<Result<Vec<String>, _>>()
            .map_err(|err| format!("cannot read the file {}: {err}", path.display()))?;

        let path_regex = Regex::new(r"^(?P<prefix>(?:export\s+)?PATH=)(?P<value>.*)$")
            .map_err(|err| format!("cannot build the PATH regular expression: {err}"))?;
        let found = lines.iter().enumerate().find_map(|(index, line)| {
            path_regex.captures(line).map(|captures| {
                (
                    index,
                    captures["prefix"].to_string(),
                    captures["value"].to_string(),
                )
            })
        });

        match found {
            Some((path_line, prefix, value)) => Ok(Self {
                lines,
                path_line,
                prefix,
                value,
            }),
            None => Err(format!(
                "no line sets the environment variable PATH in the file {}",
                path.display()
            )),
        }
    }

    /// The value of the environment variable PATH.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Rewrite the line which sets PATH with the given entries and return the
    /// whole shell configuration file.
    pub fn rewrite(&self, entries: &[String]) -> Vec<String> {
        let mut lines = self.lines.clone();
        lines[self.path_line] = format!("{}{}", self.prefix, pathlist::join(entries));
        lines
    }
}

/// Output the shell configuration file to stdout without rewriting anything.
pub fn print(lines: &[String]) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    write_lines(&mut writer, lines)
}

/// Rewrite the shell configuration file the fastest way.
pub fn write(path: &Path, lines: &[String]) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|err| format!("cannot write the file {}: {err}", path.display()))?;
    let mut writer = BufWriter::new(file);

    write_lines(&mut writer, lines)
}

fn write_lines<W: Write>(writer: &mut W, lines: &[String]) -> Result<(), String> {
    for line in lines {
        writeln!(writer, "{line}")
            .map_err(|err| format!("cannot write the shell configuration file: {err}"))?;
    }

    writer
        .flush()
        .map_err(|err| format!("cannot write the shell configuration file: {err}"))
}
