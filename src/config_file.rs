use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pathlist;

/// The line which sets the environment variable PATH in the shell
/// configuration file.
struct PathLine {
    prefix: String,
    quote: Option<char>,
    value: String,
    trailing: String,
}

impl PathLine {
    /// Rewrite the line with the given PATH entries, restoring the quoting and
    /// the trailing comment of the original line.
    fn rewrite(&self, entries: &[String]) -> String {
        let value = pathlist::join(entries);
        let value = match self.quote {
            Some(quote) => format!("{quote}{value}{quote}"),
            None => value,
        };

        format!("{}{value}{}", self.prefix, self.trailing)
    }
}

/// The shell configuration file with the line which sets the variable PATH.
pub struct ConfigFile {
    lines: Vec<String>,
    path_line: usize,
    line: PathLine,
    final_newline: bool,
}

impl ConfigFile {
    /// Read the shell configuration file and find the line which sets PATH.
    pub fn read(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|err| format!("cannot read the file {}: {err}", path.display()))?;
        let final_newline = contents.ends_with('\n');
        let lines = contents
            .lines()
            .map(str::to_string)
            .collect::<Vec<String>>();

        let found = lines
            .iter()
            .enumerate()
            .find_map(|(index, line)| parse_path_line(line).map(|parsed| (index, parsed)));

        match found {
            Some((path_line, line)) => Ok(Self {
                lines,
                path_line,
                line,
                final_newline,
            }),
            None => Err(format!(
                "no line sets the environment variable PATH in the file {}",
                path.display()
            )),
        }
    }

    /// The value of the environment variable PATH.
    pub fn value(&self) -> &str {
        &self.line.value
    }

    /// Rewrite the line which sets PATH with the given entries and return the
    /// whole shell configuration file as a string. Whether the file ends with
    /// a newline is preserved.
    pub fn rewrite(&self, entries: &[String]) -> String {
        let mut lines = self.lines.clone();
        lines[self.path_line] = self.line.rewrite(entries);

        let mut contents = lines.join("\n");

        if self.final_newline {
            contents.push('\n');
        }

        contents
    }
}

/// Output the shell configuration file to stdout without rewriting anything.
pub fn print(contents: &str) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    writer
        .write_all(contents.as_bytes())
        .map_err(|err| format!("cannot write the shell configuration file: {err}"))?;

    if !contents.ends_with('\n') {
        writer
            .write_all(b"\n")
            .map_err(|err| format!("cannot write the shell configuration file: {err}"))?;
    }

    writer
        .flush()
        .map_err(|err| format!("cannot write the shell configuration file: {err}"))
}

/// Rewrite the shell configuration file the safest way: the new content is
/// written to a temporary file in the same directory, synced, backed up on
/// demand and then renamed over the original file, which makes the replacement
/// atomic. The permissions of the original file are preserved and a brand new
/// file is created with the mode `600`.
pub fn write(path: &Path, contents: &str, backup: bool) -> Result<(), String> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "cannot write the file {}: invalid file name",
                target.display()
            )
        })?;

    let (temporary, file) = create_temporary(directory, file_name)?;

    let result = write_contents(file, &target, contents)
        .and_then(|()| backup_file(path, backup))
        .and_then(|()| rename(&temporary, &target));

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }

    result
}

/// Create a brand new temporary file in the given directory with the mode
/// `600`.
fn create_temporary(directory: &Path, file_name: &str) -> Result<(PathBuf, File), String> {
    let random = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let mut last_error = None;

    for attempt in 0..16 {
        let name = format!(
            "{file_name}.chpath.tmp.{}.{random}.{attempt}",
            process::id()
        );
        let path = directory.join(name);

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(format!("cannot write the file {}: {err}", path.display())),
        }
    }

    let error = last_error
        .map(|err| err.to_string())
        .unwrap_or_else(|| "too many attempts".to_string());

    Err(format!(
        "cannot create a temporary file in {}: {error}",
        directory.display()
    ))
}

/// Write the contents into the temporary file, preserving the permissions of
/// the target file when it exists, then flush and sync it.
fn write_contents(mut file: File, target: &Path, contents: &str) -> Result<(), String> {
    if let Ok(metadata) = fs::metadata(target) {
        file.set_permissions(metadata.permissions())
            .map_err(|err| format!("cannot write the file {}: {err}", target.display()))?;
    }

    file.write_all(contents.as_bytes())
        .map_err(|err| format!("cannot write the file {}: {err}", target.display()))?;
    file.flush()
        .map_err(|err| format!("cannot write the file {}: {err}", target.display()))?;
    file.sync_all()
        .map_err(|err| format!("cannot write the file {}: {err}", target.display()))
}

/// Copy the shell configuration file to `<path>.bak` when asked to.
fn backup_file(path: &Path, backup: bool) -> Result<(), String> {
    if !backup || !path.exists() {
        return Ok(());
    }

    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    let backup = PathBuf::from(name);

    fs::copy(path, &backup)
        .map(|_| ())
        .map_err(|err| format!("cannot write the backup file {}: {err}", backup.display()))
}

/// Rename the temporary file over the target file, which replaces it
/// atomically.
fn rename(temporary: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temporary, target)
        .map_err(|err| format!("cannot write the file {}: {err}", target.display()))
}

/// Find the prefix `PATH=` or `export PATH=` at the start of the line and parse
/// the rest of the line.
fn parse_path_line(line: &str) -> Option<PathLine> {
    let rest = match line.strip_prefix("export") {
        Some(rest) => rest.trim_start(),
        None => line,
    };

    let rest = rest.strip_prefix("PATH=")?;
    let prefix = &line[..line.len() - rest.len()];
    let (quote, value, trailing) = split_value(rest);

    Some(PathLine {
        prefix: prefix.to_string(),
        quote,
        value,
        trailing,
    })
}

/// Split a PATH value from its optional quoting and its optional trailing
/// inline comment.
fn split_value(rest: &str) -> (Option<char>, String, String) {
    if let Some(quote) = rest
        .chars()
        .next()
        .filter(|char| *char == '\'' || *char == '"')
    {
        let inner = &rest[quote.len_utf8()..];

        if let Some(end) = inner.find(quote) {
            let value = inner[..end].to_string();
            let trailing = inner[end + quote.len_utf8()..].to_string();

            return (Some(quote), value, trailing);
        }

        return (None, rest.to_string(), String::new());
    }

    match comment_start(rest) {
        Some(index) => {
            let value = rest[..index].trim_end().to_string();
            let trailing = rest[value.len()..].to_string();

            (None, value, trailing)
        }
        None => (None, rest.trim_end().to_string(), String::new()),
    }
}

/// Find the index of the inline comment of an unquoted PATH value, if any.
fn comment_start(value: &str) -> Option<usize> {
    value.char_indices().find_map(|(index, char)| {
        let comment = char == '#' && (index == 0 || value[..index].ends_with(char::is_whitespace));

        comment.then_some(index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temporary_directory() -> PathBuf {
        let random = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let directory =
            std::env::temp_dir().join(format!("chpath-test-{}-{random}", process::id()));

        fs::create_dir_all(&directory).expect("temporary directory is created");

        directory
    }

    #[test]
    fn parse_path_line_handles_a_plain_assignment() {
        let line = parse_path_line("PATH=/usr/bin:/usr/lib").expect("line is parsed");

        assert_eq!(line.prefix, "PATH=");
        assert_eq!(line.quote, None);
        assert_eq!(line.value, "/usr/bin:/usr/lib");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn parse_path_line_handles_export() {
        let line = parse_path_line("export PATH=$PATH:/usr/bin").expect("line is parsed");

        assert_eq!(line.prefix, "export PATH=");
        assert_eq!(line.value, "$PATH:/usr/bin");
    }

    #[test]
    fn parse_path_line_handles_double_quotes() {
        let line = parse_path_line("export PATH=\"$HOME/bin:$PATH\"").expect("line is parsed");

        assert_eq!(line.quote, Some('"'));
        assert_eq!(line.value, "$HOME/bin:$PATH");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn parse_path_line_handles_a_trailing_comment() {
        let line =
            parse_path_line("export PATH=/usr/bin:$PATH  # my tools").expect("line is parsed");

        assert_eq!(line.value, "/usr/bin:$PATH");
        assert_eq!(line.trailing, "  # my tools");
    }

    #[test]
    fn parse_path_line_keeps_a_hash_inside_the_path() {
        let line = parse_path_line("PATH=/usr/bin#tag").expect("line is parsed");

        assert_eq!(line.value, "/usr/bin#tag");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn rewrite_restores_the_quotes_and_the_comment() {
        let line = parse_path_line("export PATH=\"$HOME/bin\"  # mine").expect("line is parsed");
        let entries = vec!["/opt/bin".to_string()];

        assert_eq!(line.rewrite(&entries), "export PATH=\"/opt/bin\"  # mine");
    }

    #[test]
    fn write_creates_a_backup() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "export PATH=/usr/bin\n").expect("file is written");

        write(&path, "export PATH=/usr/bin:/opt/bin\n", true).expect("file is rewritten");

        assert_eq!(
            fs::read_to_string(directory.join(".bashrc.bak")).expect("backup is read"),
            "export PATH=/usr/bin\n"
        );
        assert_eq!(
            fs::read_to_string(&path).expect("file is read"),
            "export PATH=/usr/bin:/opt/bin\n"
        );

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn write_skips_the_backup_without_the_flag() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "export PATH=/usr/bin\n").expect("file is written");

        write(&path, "export PATH=/usr/bin:/opt/bin\n", false).expect("file is rewritten");

        assert!(!directory.join(".bashrc.bak").exists());

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn write_preserves_the_permissions() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "export PATH=/usr/bin\n").expect("file is written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("permissions are set");

        write(&path, "export PATH=/opt/bin\n", false).expect("file is rewritten");

        let mode = fs::metadata(&path)
            .expect("metadata is read")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(mode, 0o600);

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }
}
