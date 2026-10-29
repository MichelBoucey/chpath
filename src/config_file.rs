use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pathlist;

/// The line which sets the environment variable PATH in the shell
/// configuration file.
#[derive(Debug)]
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
    line_ending: &'static str,
    final_newline: bool,
}

impl ConfigFile {
    /// Read the shell configuration file and find the single line which sets
    /// PATH.
    ///
    /// Fails when no line sets PATH, when several lines set it, and when the
    /// line setting PATH uses a syntax which chpath does not support (`PATH+=`
    /// or a continuation).
    pub fn read(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|err| format!("cannot read the file {}: {err}", path.display()))?;
        let line_ending = if contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let final_newline = contents.ends_with('\n');
        let lines = contents
            .lines()
            .map(str::to_string)
            .collect::<Vec<String>>();

        let mut found = Vec::new();

        for (index, line) in lines.iter().enumerate() {
            match parse_path_line(line) {
                Parsed::Match(line) => found.push((index, line)),
                Parsed::Continuation => {
                    return Err(format!(
                        "the line {} setting the environment variable PATH in the file {} \
                         continues on the next line, which chpath does not support",
                        index + 1,
                        path.display()
                    ));
                }
                Parsed::Append => {
                    return Err(format!(
                        "the line {} appends to the environment variable PATH in the file {} \
                         with 'PATH+=', which chpath does not support",
                        index + 1,
                        path.display()
                    ));
                }
                Parsed::Ignore => {}
            }
        }

        if found.len() > 1 {
            let numbers = found
                .iter()
                .map(|(index, _)| (index + 1).to_string())
                .collect::<Vec<String>>()
                .join(", ");

            return Err(format!(
                "several lines set the environment variable PATH in the file {}: lines {numbers}",
                path.display()
            ));
        }

        match found.pop() {
            Some((path_line, line)) => Ok(Self {
                lines,
                path_line,
                line,
                line_ending,
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

        let mut contents = lines.join(self.line_ending);

        if self.final_newline {
            contents.push_str(self.line_ending);
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

/// What a line of the shell configuration file holds about PATH.
#[derive(Debug)]
enum Parsed {
    /// The line sets PATH and chpath can rewrite it.
    Match(PathLine),
    /// The line sets PATH but continues on the next line with `\`.
    Continuation,
    /// The line appends to PATH with `PATH+=`.
    Append,
    /// The line has nothing to do with PATH.
    Ignore,
}

/// Find the prefix `PATH=`, `export PATH=`, `declare -x PATH=` or
/// `typeset -x PATH=` at the start of the line and parse the rest of the line.
///
/// The indentation of the line is kept in the prefix, as well as the prefix
/// itself and the spaces which follow the keyword. The spaces after the `=`
/// are ignored.
fn parse_path_line(line: &str) -> Parsed {
    let trimmed = line.trim_start();
    let rest = ["export ", "declare -x ", "typeset -x "]
        .iter()
        .find_map(|prefix| trimmed.strip_prefix(prefix))
        .map_or(trimmed, str::trim_start);

    let Some(value) = rest.strip_prefix("PATH=") else {
        if rest.starts_with("PATH+=") {
            return Parsed::Append;
        }

        return Parsed::Ignore;
    };

    let value = value.trim_start();
    let prefix = line[..line.len() - value.len()].trim_end();
    let (quote, value, trailing) = split_value(value);

    if quote.is_none() && value.trim_end().ends_with('\\') {
        return Parsed::Continuation;
    }

    Parsed::Match(PathLine {
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

    fn parse(line: &str) -> PathLine {
        match parse_path_line(line) {
            Parsed::Match(line) => line,
            other => panic!("the line is not parsed: {other:?}"),
        }
    }

    #[test]
    fn parse_path_line_handles_a_plain_assignment() {
        let line = parse("PATH=/usr/bin:/usr/lib");

        assert_eq!(line.prefix, "PATH=");
        assert_eq!(line.quote, None);
        assert_eq!(line.value, "/usr/bin:/usr/lib");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn parse_path_line_handles_export() {
        let line = parse("export PATH=$PATH:/usr/bin");

        assert_eq!(line.prefix, "export PATH=");
        assert_eq!(line.value, "$PATH:/usr/bin");
    }

    #[test]
    fn parse_path_line_handles_declare_and_typeset() {
        assert_eq!(
            parse("declare -x PATH=\"/usr/bin\"").prefix,
            "declare -x PATH="
        );
        assert_eq!(parse("typeset -x PATH=/usr/bin").prefix, "typeset -x PATH=");
        assert_eq!(parse("export   PATH=/usr/bin").prefix, "export   PATH=");
    }

    #[test]
    fn parse_path_line_keeps_the_indentation() {
        let line = parse("  export PATH=/usr/bin");

        assert_eq!(line.prefix, "  export PATH=");
        assert_eq!(line.value, "/usr/bin");
    }

    #[test]
    fn parse_path_line_drops_the_spaces_after_the_equal_sign() {
        let line = parse("PATH=  /usr/bin:/opt/bin");

        assert_eq!(line.prefix, "PATH=");
        assert_eq!(line.value, "/usr/bin:/opt/bin");
    }

    #[test]
    fn parse_path_line_handles_double_quotes() {
        let line = parse("export PATH=\"$HOME/bin:$PATH\"");

        assert_eq!(line.quote, Some('"'));
        assert_eq!(line.value, "$HOME/bin:$PATH");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn parse_path_line_handles_a_trailing_comment() {
        let line = parse("export PATH=/usr/bin:$PATH  # my tools");

        assert_eq!(line.value, "/usr/bin:$PATH");
        assert_eq!(line.trailing, "  # my tools");
    }

    #[test]
    fn parse_path_line_keeps_a_hash_inside_the_path() {
        let line = parse("PATH=/usr/bin#tag");

        assert_eq!(line.value, "/usr/bin#tag");
        assert_eq!(line.trailing, "");
    }

    #[test]
    fn parse_path_line_detects_a_continuation() {
        assert!(matches!(
            parse_path_line("export PATH=/usr/bin:\\"),
            Parsed::Continuation
        ));
        assert!(matches!(
            parse_path_line("PATH=/usr/bin/\t\\"),
            Parsed::Continuation
        ));
        assert!(matches!(
            parse_path_line("export PATH=/usr/bin  # note \\"),
            Parsed::Match(_)
        ));
        assert!(matches!(
            parse_path_line("PATH=\"/usr/bin/\""),
            Parsed::Match(_)
        ));
    }

    #[test]
    fn parse_path_line_detects_an_append() {
        assert!(matches!(parse_path_line("PATH+=:/opt/bin"), Parsed::Append));
        assert!(matches!(
            parse_path_line("export PATH+=:/opt/bin"),
            Parsed::Append
        ));
    }

    #[test]
    fn parse_path_line_ignores_other_lines() {
        assert!(matches!(parse_path_line("export FOO=bar"), Parsed::Ignore));
        assert!(matches!(parse_path_line("# PATH=/usr/bin"), Parsed::Ignore));
        assert!(matches!(parse_path_line("export PATH"), Parsed::Ignore));
    }

    #[test]
    fn rewrite_restores_the_quotes_and_the_comment() {
        let line = parse("export PATH=\"$HOME/bin\"  # mine");
        let entries = vec!["/opt/bin".to_string()];

        assert_eq!(line.rewrite(&entries), "export PATH=\"/opt/bin\"  # mine");
    }

    #[test]
    fn read_fails_on_several_lines_setting_path() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "PATH=/first\nPATH=/second\n").expect("file is written");

        let error = ConfigFile::read(&path).err().expect("reading fails");

        assert!(error.contains("lines 1, 2"), "unexpected error: {error}");

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn read_fails_on_a_continuation_and_an_append() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");

        fs::write(&path, "export PATH=/usr/bin:\\\n  /opt/bin\n").expect("file is written");
        let error = ConfigFile::read(&path).err().expect("reading fails");
        assert!(
            error.contains("continues on the next line"),
            "unexpected error: {error}"
        );

        fs::write(&path, "PATH+=:/opt/bin\n").expect("file is written");
        let error = ConfigFile::read(&path).err().expect("reading fails");
        assert!(error.contains("PATH+="), "unexpected error: {error}");

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn read_fails_without_a_line_setting_path() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "export FOO=bar\n").expect("file is written");

        let error = ConfigFile::read(&path).err().expect("reading fails");

        assert!(error.contains("no line sets"), "unexpected error: {error}");

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn rewrite_preserves_the_crlf_line_endings() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "export PATH=/usr/bin\r\nexport FOO=bar\r\n").expect("file is written");

        let config = ConfigFile::read(&path).expect("file is read");
        let contents = config.rewrite(&["/opt/bin".to_string()]);

        assert_eq!(contents, "export PATH=/opt/bin\r\nexport FOO=bar\r\n");

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
    }

    #[test]
    fn rewrite_keeps_the_indentation_of_the_line() {
        let directory = temporary_directory();
        let path = directory.join(".bashrc");
        fs::write(&path, "if true; then\n  declare -x PATH=\"/usr/bin\"\nfi\n")
            .expect("file is written");

        let config = ConfigFile::read(&path).expect("file is read");
        let contents = config.rewrite(&["/opt/bin".to_string()]);

        assert_eq!(
            contents,
            "if true; then\n  declare -x PATH=\"/opt/bin\"\nfi\n"
        );

        fs::remove_dir_all(&directory).expect("temporary directory is removed");
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
