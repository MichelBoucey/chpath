use std::io::{self, Write};

use crate::pathlist;

/// The shell command which sets PATH in the current shell session.
pub fn line(entries: &[String]) -> String {
    format!("export PATH={}\n", quote(&pathlist::join(entries)))
}

/// Output the shell command which sets PATH in the current shell session.
pub fn print(entries: &[String]) -> Result<(), String> {
    let line = line(entries);
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    writer
        .write_all(line.as_bytes())
        .and_then(|()| writer.flush())
        .map_err(|err| format!("cannot write the shell command: {err}"))
}

/// Quote a value so that the shell reads it verbatim inside single quotes.
///
/// A single quote is closed, escaped and reopened, so a value holding a space,
/// a quote or a newline can be stored in the exported PATH safely.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_wraps_the_value_in_single_quotes() {
        assert_eq!(quote("/usr/bin:/bin"), "'/usr/bin:/bin'");
    }

    #[test]
    fn quote_escapes_the_single_quotes() {
        assert_eq!(quote("/opt/it's/bin"), "'/opt/it'\\''s/bin'");
    }

    #[test]
    fn line_exports_the_joined_entries() {
        let entries = vec!["/usr/bin".to_string(), "/bin".to_string()];

        assert_eq!(line(&entries), "export PATH='/usr/bin:/bin'\n");
    }

    #[test]
    fn line_exports_an_empty_path() {
        assert_eq!(line(&[]), "export PATH=''\n");
    }
}
