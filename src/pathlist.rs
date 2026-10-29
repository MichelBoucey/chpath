use std::collections::HashSet;
use std::path::Path;

/// The characters which are stored verbatim in the shell configuration file.
const SAFE_CHARS: &str = "/._-+@,=%";

/// Why a path cannot be removed from PATH.
#[derive(Debug, PartialEq, Eq)]
pub enum RemoveError {
    /// The path is not present in PATH at all.
    Missing,
    /// The path only comes from the live PATH, through the pseudo-entry
    /// `$PATH`, and thus cannot be removed from the configuration file.
    Inherited,
}

impl RemoveError {
    /// The sentence describing the failure, output by `--remove`.
    pub fn describe(&self, path: &str, config: &Path) -> String {
        match self {
            RemoveError::Missing => format!("the path '{path}' is not present in PATH"),
            RemoveError::Inherited => format!(
                "the path '{path}' is inherited from the live PATH through the $PATH pseudo-entry \
                 and cannot be removed from the file {}, which it does not set",
                config.display()
            ),
        }
    }
}

/// An issue detected in the PATH value.
#[derive(Debug, PartialEq, Eq)]
pub enum Issue {
    Dead(String),
    Duplicate(String),
}

impl Issue {
    /// The sentence describing the issue, output by `--check` and `--cleanup`.
    pub fn describe(&self) -> String {
        match self {
            Issue::Dead(path) => format!("the directory path '{path}' no longer exists"),
            Issue::Duplicate(path) => format!("the path '{path}' is duplicated in PATH"),
        }
    }
}

/// The result of a cleanup: the kept entries, the dead entries and the
/// duplicated entries.
pub struct Cleanup {
    pub kept: Vec<String>,
    pub dead: Vec<String>,
    pub duplicates: Vec<String>,
}

impl Cleanup {
    /// The issues detected by the cleanup: the dead entries first, then the
    /// duplicated ones.
    pub fn issues(&self) -> Vec<Issue> {
        self.dead
            .iter()
            .cloned()
            .map(Issue::Dead)
            .chain(self.duplicates.iter().cloned().map(Issue::Duplicate))
            .collect()
    }
}

/// Append `path` to the given PATH entries unless it is already present.
///
/// The entries are compared in their normalized form (see `key`), so `--add`
/// is idempotent even when the path is written with a trailing slash or with a
/// `$HOME` prefix. Returns the resulting entries and `true` when the path has
/// been added.
pub fn add(entries: &[String], path: &str, home: &str) -> (Vec<String>, bool) {
    let target = key(path, home);

    if entries.iter().any(|entry| key(entry, home) == target) {
        return (entries.to_vec(), false);
    }

    if entries.iter().any(|entry| entry == "$PATH") && in_live_path(&target) {
        return (entries.to_vec(), false);
    }

    let mut updated = entries.to_vec();
    updated.push(path.to_string());
    (updated, true)
}

/// Remove `path` from the given PATH entries.
///
/// The entries are compared in their normalized form (see `key`). Fails when
/// the path is not present in PATH, telling apart a path which is absent and a
/// path which is only inherited from the live PATH through `$PATH`.
pub fn remove(entries: &[String], path: &str, home: &str) -> Result<Vec<String>, RemoveError> {
    let target = key(path, home);

    if !entries.iter().any(|entry| key(entry, home) == target) {
        let inherited = entries.iter().any(|entry| entry == "$PATH") && in_live_path(&target);

        return Err(if inherited {
            RemoveError::Inherited
        } else {
            RemoveError::Missing
        });
    }

    Ok(entries
        .iter()
        .filter(|entry| key(entry, home) != target)
        .cloned()
        .collect())
}

/// Split the value of PATH on the colon separator.
pub fn parse(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }

    value
        .split(':')
        .map(str::to_string)
        .collect::<Vec<String>>()
}

/// Join the given PATH entries on the colon separator.
pub fn join(entries: &[String]) -> String {
    entries.join(":")
}

/// Tell if the directory of a PATH entry no longer exists in the file system.
///
/// The prefixes `$HOME`, `${HOME}` and `~` are expanded before the check. The
/// item `$PATH`, which concatenates the old value of PATH, the empty items and
/// the items holding an environment variable reference that chpath cannot
/// expand are never checked.
pub fn is_dead(entry: &str, home: &str) -> bool {
    if entry.is_empty() || entry == "$PATH" {
        return false;
    }

    let expanded = expand(entry, home);

    if has_unresolved_var(&expanded) {
        return false;
    }

    !Path::new(&expanded).is_dir()
}

/// Tell if a PATH entry is an absolute path.
///
/// The prefixes `$HOME`, `${HOME}` and `~` are expanded first, so `~/.go/bin`
/// is absolute.
pub fn is_absolute(entry: &str, home: &str) -> bool {
    expand(entry, home).starts_with('/')
}

/// Check that a path can be added to PATH and stored safely in the shell
/// configuration file.
///
/// The path must be absolute and must not hold any character which could break
/// the shell configuration file when written in it (a space, a quote, a
/// newline...). The directory must also exist unless `force` is set, or unless
/// the path holds an environment variable reference which chpath cannot
/// resolve.
pub fn validate_add(path: &str, home: &str, force: bool) -> Result<(), String> {
    if path.is_empty() {
        return Err("the path to add is empty".to_string());
    }

    if !is_absolute(path, home) {
        return Err(format!(
            "the path '{path}' is not absolute; every PATH entry must be an absolute path or \
             start with $HOME, ${{HOME}} or ~"
        ));
    }

    if let Some(char) = unsafe_char(path) {
        return Err(format!(
            "cannot add the path '{path}': the character {char:?} cannot be written safely in \
             the shell configuration file, add it manually"
        ));
    }

    let expanded = expand(path, home);

    if !force && !has_unresolved_var(&expanded) && !Path::new(&expanded).is_dir() {
        return Err(format!(
            "the directory '{path}' does not exist, use --force to add it anyway"
        ));
    }

    Ok(())
}

/// The first character of a PATH entry which forbids storing it in the shell
/// configuration file, if any.
///
/// The characters are stored verbatim in the line which sets PATH, so a space
/// or a quote would break the line and a newline would inject a whole line in
/// the file. Variable references `$NAME` and `${NAME}`, which the shell
/// expands at startup, and a leading `~` are accepted.
pub fn unsafe_char(entry: &str) -> Option<char> {
    let mut rest = entry;

    if let Some(after) = rest.strip_prefix('~')
        && (after.is_empty() || after.starts_with('/'))
    {
        rest = after;
    }

    let mut index = 0;

    while let Some(char) = rest[index..].chars().next() {
        if char.is_control() {
            return Some(char);
        }

        if char == '$' {
            match variable_reference_len(&rest[index..]) {
                Some(len) => {
                    index += len;

                    continue;
                }
                None => return Some(char),
            }
        }

        if char.is_alphanumeric() || SAFE_CHARS.contains(char) {
            index += char.len_utf8();

            continue;
        }

        return Some(char);
    }

    None
}

/// The length in bytes of the variable reference `$NAME` or `${NAME}` which
/// starts the given text, when it is well formed.
fn variable_reference_len(text: &str) -> Option<usize> {
    let after = text.strip_prefix('$')?;

    if let Some(braced) = after.strip_prefix('{') {
        let name = braced.split_once('}')?.0;
        let len = variable_name_len(name)?;

        return (len == name.len()).then(|| 2 + name.len() + 1);
    }

    variable_name_len(after).map(|len| 1 + len)
}

/// The length in bytes of the variable name at the start of the given text.
fn variable_name_len(text: &str) -> Option<usize> {
    let mut len = 0;

    for (index, char) in text.char_indices() {
        let ident = char.is_ascii_alphanumeric() || char == '_';
        let start = char.is_ascii_alphabetic() || char == '_';

        if ident && (index != 0 || start) {
            len = index + char.len_utf8();
        } else {
            break;
        }
    }

    (len > 0).then_some(len)
}

/// Drop the duplicate PATH entries, keeping the first occurrence.
///
/// The entries are compared in their normalized form (see `key`). The item
/// `$PATH` and the empty items are kept as is and never considered duplicates.
/// Returns the kept entries and the duplicated entries removed, verbatim.
pub fn dedup(entries: &[String], home: &str) -> (Vec<String>, Vec<String>) {
    let (kept, duplicates, _) = entries.iter().fold(
        (Vec::new(), Vec::new(), HashSet::new()),
        |(mut kept, mut duplicates, mut seen), entry| {
            if entry.is_empty() || entry == "$PATH" {
                kept.push(entry.clone());

                return (kept, duplicates, seen);
            }

            if seen.insert(key(entry, home)) {
                kept.push(entry.clone());
            } else {
                duplicates.push(entry.clone());
            }

            (kept, duplicates, seen)
        },
    );

    (kept, duplicates)
}

/// Remove the dead and the duplicated PATH entries.
///
/// The dead entries are removed first, then the duplicates of the surviving
/// entries are dropped keeping the first occurrence.
pub fn cleanup(entries: &[String], home: &str) -> Cleanup {
    let (survivors, dead): (Vec<String>, Vec<String>) = entries
        .iter()
        .cloned()
        .partition(|entry| !is_dead(entry, home));

    let (kept, duplicates) = dedup(&survivors, home);

    Cleanup {
        kept,
        dead,
        duplicates,
    }
}

/// The issues detected in PATH, exactly as `--cleanup` fixes them: the dead
/// entries first, then the duplicated ones.
pub fn issues(entries: &[String], home: &str) -> Vec<Issue> {
    cleanup(entries, home).issues()
}

/// Output the PATH entries which are real paths, expanding the pseudo-entry
/// `$PATH` with the live value of the environment variable PATH, in place.
///
/// The empty entries are skipped and the duplicates are dropped keeping the
/// first occurrence, compared in their normalized form (see `key`), exactly as
/// `--cleanup` does.
pub fn visible_paths(entries: &[String], live_path: &str, home: &str) -> Vec<String> {
    let expanded = entries
        .iter()
        .flat_map(|entry| match entry.as_str() {
            "$PATH" => parse(live_path),
            "" => Vec::new(),
            other => vec![other.to_string()],
        })
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<String>>();

    let (kept, _) = dedup(&expanded, home);

    kept
}

/// The normalized form of a PATH entry, used to compare entries for equality.
///
/// The prefixes `$HOME`, `${HOME}` and `~` are expanded, the repeated slashes
/// are collapsed and the trailing slash is dropped, so `/usr/bin/` and
/// `/usr/bin` compare equal.
fn key(entry: &str, home: &str) -> String {
    let expanded = expand(entry, home);
    let mut normalized = String::with_capacity(expanded.len());
    let mut previous_slash = false;

    for char in expanded.chars() {
        if char == '/' {
            if !previous_slash {
                normalized.push(char);
            }

            previous_slash = true;
        } else {
            normalized.push(char);
            previous_slash = false;
        }
    }

    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

/// Expand the prefixes `$HOME`, `${HOME}` and `~` of a PATH entry.
fn expand(entry: &str, home: &str) -> String {
    for prefix in ["$HOME/", "${HOME}/", "~/"] {
        if let Some(rest) = entry.strip_prefix(prefix) {
            return format!("{home}/{rest}");
        }
    }

    for exact in ["$HOME", "${HOME}", "~"] {
        if entry == exact {
            return home.to_string();
        }
    }

    entry.to_string()
}

fn normalize_segment(segment: &str) -> String {
    let mut normalized = String::with_capacity(segment.len());
    let mut previous_slash = false;

    for char in segment.chars() {
        if char == '/' {
            if !previous_slash {
                normalized.push(char);
            }

            previous_slash = true;
        } else {
            normalized.push(char);
            previous_slash = false;
        }
    }

    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

fn in_live_path(target_key: &str) -> bool {
    if let Ok(live) = std::env::var("PATH") {
        for segment in live.split(':') {
            if segment.is_empty() {
                continue;
            }

            if has_unresolved_var(segment) {
                continue;
            }

            if normalize_segment(segment) == target_key {
                return true;
            }
        }
    }

    false
}

/// Tell if the entry still holds an environment variable reference which
/// chpath cannot expand.
fn has_unresolved_var(entry: &str) -> bool {
    let mut chars = entry.chars().peekable();

    while let Some(char) = chars.next() {
        if char == '$' {
            match chars.peek() {
                Some('{') => return true,
                Some(next) if next.is_ascii_alphabetic() || *next == '_' => return true,
                _ => {}
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_appends_a_new_path() {
        let entries = parse("/usr/bin:/usr/lib");

        let (updated, added) = add(&entries, "/opt/bin", "/home/user");

        assert!(added);
        assert_eq!(updated, vec!["/usr/bin", "/usr/lib", "/opt/bin"]);
    }

    #[test]
    fn add_is_idempotent() {
        let entries = parse("/usr/bin:/usr/lib");

        let (updated, added) = add(&entries, "/usr/bin", "/home/user");

        assert!(!added);
        assert_eq!(updated, entries);
    }

    #[test]
    fn add_is_idempotent_on_a_trailing_slash() {
        let entries = parse("/usr/bin");

        let (updated, added) = add(&entries, "/usr/bin/", "/home/user");

        assert!(!added);
        assert_eq!(updated, entries);
    }

    #[test]
    fn add_is_idempotent_on_a_home_prefix() {
        let entries = parse("$HOME/.go/bin:/usr/bin");

        let (_, added) = add(&entries, "~/.go/bin", "/home/user");

        assert!(!added);
    }

    #[test]
    fn remove_drops_the_given_path() {
        let entries = parse("/usr/bin:/usr/lib:/opt/bin");

        let updated = remove(&entries, "/usr/lib", "/home/user").expect("path is present");

        assert_eq!(updated, vec!["/usr/bin", "/opt/bin"]);
    }

    #[test]
    fn remove_matches_a_normalized_path() {
        let entries = parse("/usr/bin:$HOME/.go/bin");

        let updated = remove(&entries, "/usr/bin/", "/home/user").expect("path is present");

        assert_eq!(updated, vec!["$HOME/.go/bin"]);
    }

    #[test]
    fn remove_fails_on_an_absent_path() {
        let entries = parse("/usr/bin");

        assert_eq!(
            remove(&entries, "/definitely/not/in/path", "/home/user"),
            Err(RemoveError::Missing)
        );
    }

    #[test]
    fn remove_describes_the_inherited_case() {
        let message = RemoveError::Inherited.describe("/bin", Path::new("/home/user/.bashrc"));

        assert!(
            message.contains("inherited"),
            "unexpected message: {message}"
        );
        assert!(message.contains("/home/user/.bashrc"));
    }

    #[test]
    fn validate_add_accepts_an_existing_directory() {
        assert!(validate_add("/usr/bin", "/home/user", false).is_ok());
    }

    #[test]
    fn validate_add_checks_the_directory_after_expanding_home() {
        let error = validate_add("~/.go/bin", "/definitely/not/a/home", false)
            .expect_err("path is refused");

        assert!(
            error.contains("does not exist"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_add_skips_the_existence_check_on_a_variable() {
        assert!(validate_add("/opt/$XDG_BIN_HOME/bin", "/home/user", false).is_ok());
    }

    #[test]
    fn validate_add_rejects_an_empty_path() {
        assert_eq!(
            validate_add("", "/home/user", false),
            Err("the path to add is empty".to_string())
        );
    }

    #[test]
    fn validate_add_rejects_a_relative_path() {
        let error = validate_add("bin/tools", "/home/user", false).expect_err("path is refused");

        assert!(
            error.contains("is not absolute"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_add_rejects_a_missing_directory() {
        let error = validate_add("/definitely/not/a/directory", "/home/user", false)
            .expect_err("path is refused");

        assert!(
            error.contains("does not exist"),
            "unexpected error: {error}"
        );
        assert!(validate_add("/definitely/not/a/directory", "/home/user", true).is_ok());
    }

    #[test]
    fn validate_add_rejects_an_unsafe_path() {
        for path in [
            "/opt/my tools",
            "/opt/it's/bin",
            "/opt/back\\slash",
            "/opt/tag#1",
            "/tmp/x\nexport EVIL=1",
            "/opt/$",
        ] {
            let error = match validate_add(path, "/home/user", false) {
                Ok(()) => panic!("the path {path:?} is accepted"),
                Err(error) => error,
            };

            assert!(
                error.contains("cannot be written safely"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn unsafe_char_accepts_safe_entries() {
        for entry in [
            "/usr/bin",
            "/usr//bin/",
            "$HOME/.go/bin",
            "${HOME}/.go/bin",
            "~/.go/bin",
            "/opt/$XDG_BIN_HOME/bin",
            "/opt/${XDG_BIN_HOME}/bin",
            "/opt/café/bin",
            "/opt/bin_1.2+d",
        ] {
            assert_eq!(unsafe_char(entry), None, "the entry {entry:?} is refused");
        }
    }

    #[test]
    fn unsafe_char_refuses_unsafe_entries() {
        for (entry, char) in [
            ("/opt/my tools", ' '),
            ("/opt/it's/bin", '\''),
            ("/opt/\"quoted\"/bin", '"'),
            ("/opt/back\\slash", '\\'),
            ("/opt/tag#1", '#'),
            ("/opt/a:/b", ':'),
            ("/opt/bin;rm -rf", ';'),
            ("/tmp/x\nexport EVIL=1", '\n'),
            ("/tmp/x\tevil", '\t'),
            ("/opt/$", '$'),
            ("/opt/${HOME", '$'),
            ("/opt/$9/bin", '$'),
            ("/opt/~/bin", '~'),
            ("~user/bin", '~'),
        ] {
            assert_eq!(
                unsafe_char(entry),
                Some(char),
                "the entry {entry:?} is accepted"
            );
        }
    }

    #[test]
    fn is_absolute_expands_the_home_prefixes() {
        assert!(is_absolute("/usr/bin", "/home/user"));
        assert!(is_absolute("$HOME/.go/bin", "/home/user"));
        assert!(is_absolute("${HOME}/.go/bin", "/home/user"));
        assert!(is_absolute("~/.go/bin", "/home/user"));
        assert!(!is_absolute("bin", "/home/user"));
        assert!(!is_absolute("./bin", "/home/user"));
        assert!(!is_absolute("$XDG_BIN_HOME/bin", "/home/user"));
    }

    #[test]
    fn dedup_keeps_the_first_occurrence() {
        let entries = parse("/usr/bin:/opt/bin:/usr/bin:/usr/bin/");

        let (kept, duplicates) = dedup(&entries, "/home/user");

        assert_eq!(kept, vec!["/usr/bin", "/opt/bin"]);
        assert_eq!(duplicates, vec!["/usr/bin", "/usr/bin/"]);
    }

    #[test]
    fn dedup_keeps_the_path_item_and_empty_entries() {
        let entries = parse("$PATH::$PATH:");

        let (kept, duplicates) = dedup(&entries, "/home/user");

        assert_eq!(kept, vec!["$PATH", "", "$PATH", ""]);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn cleanup_removes_only_the_dead_paths() {
        let entries = parse("/definitely/not/a/directory:/tmp");

        let cleanup = cleanup(&entries, "/home/user");

        assert_eq!(cleanup.kept, vec!["/tmp"]);
        assert_eq!(cleanup.dead, vec!["/definitely/not/a/directory"]);
        assert!(cleanup.duplicates.is_empty());
    }

    #[test]
    fn cleanup_removes_the_duplicates_of_the_surviving_paths() {
        let entries = parse("/tmp:/tmp:/usr/bin");

        let cleanup = cleanup(&entries, "/home/user");

        assert_eq!(cleanup.kept, vec!["/tmp", "/usr/bin"]);
        assert!(cleanup.dead.is_empty());
        assert_eq!(cleanup.duplicates, vec!["/tmp"]);
    }

    #[test]
    fn cleanup_removes_a_dead_duplicate_once_as_dead() {
        let entries = parse("/definitely/not/a/directory:/definitely/not/a/directory:/tmp");

        let cleanup = cleanup(&entries, "/home/user");

        assert_eq!(cleanup.kept, vec!["/tmp"]);
        assert_eq!(
            cleanup.dead,
            vec!["/definitely/not/a/directory", "/definitely/not/a/directory"]
        );
        assert!(cleanup.duplicates.is_empty());
    }

    #[test]
    fn issues_reports_the_dead_paths_then_the_duplicates() {
        let entries = parse("/tmp:/tmp:/definitely/not/a/directory");

        let issues = issues(&entries, "/home/user");

        assert_eq!(
            issues,
            vec![
                Issue::Dead("/definitely/not/a/directory".to_string()),
                Issue::Duplicate("/tmp".to_string()),
            ]
        );
    }

    #[test]
    fn issues_is_empty_on_a_clean_path() {
        let entries = parse("/tmp:/usr/bin:$PATH");

        let issues = issues(&entries, "/home/user");

        assert!(issues.is_empty());
    }

    #[test]
    fn issue_describes_the_dead_and_duplicate_cases() {
        assert_eq!(
            Issue::Dead("/old".to_string()).describe(),
            "the directory path '/old' no longer exists"
        );
        assert_eq!(
            Issue::Duplicate("/usr/bin".to_string()).describe(),
            "the path '/usr/bin' is duplicated in PATH"
        );
    }

    #[test]
    fn is_dead_skips_unresolved_variables() {
        assert!(!is_dead("$XDG_BIN_HOME/bin", "/definitely/not/a/home"));
        assert!(!is_dead("${FOO}/bin", "/definitely/not/a/home"));
        assert!(is_dead("$HOME/.old/tools", "/definitely/not/a/home"));
    }

    #[test]
    fn visible_paths_expands_the_path_item_in_place() {
        let entries = parse("/usr/bin:$PATH:/opt/bin");

        let visible = visible_paths(&entries, "/sbin:/bin", "/home/user");

        assert_eq!(visible, vec!["/usr/bin", "/sbin", "/bin", "/opt/bin"]);
    }

    #[test]
    fn visible_paths_skips_the_empty_entries_and_live_segments() {
        let entries = parse("/usr/bin::$PATH::/opt/bin");

        let visible = visible_paths(&entries, "/sbin::/bin", "/home/user");

        assert_eq!(visible, vec!["/usr/bin", "/sbin", "/bin", "/opt/bin"]);
    }

    #[test]
    fn visible_paths_drops_the_normalized_duplicates() {
        let entries = parse("$HOME/.local/bin:$PATH:/usr/bin:/opt/bin");

        let visible = visible_paths(&entries, "/home/user/.local/bin/:/usr/bin", "/home/user");

        assert_eq!(visible, vec!["$HOME/.local/bin", "/usr/bin", "/opt/bin"]);
    }

    #[test]
    fn visible_paths_keeps_the_entries_verbatim() {
        let entries = parse("$HOME/.go/bin:~/tools");

        let visible = visible_paths(&entries, "", "/home/user");

        assert_eq!(visible, vec!["$HOME/.go/bin", "~/tools"]);
    }

    #[test]
    fn visible_paths_is_empty_without_a_live_path() {
        let entries = parse("$PATH");

        let visible = visible_paths(&entries, "", "/home/user");

        assert!(visible.is_empty());
    }

    #[test]
    fn expand_deals_with_home_prefixes() {
        assert_eq!(expand("$HOME/.go/bin", "/home/user"), "/home/user/.go/bin");
        assert_eq!(expand("~/bin", "/home/user"), "/home/user/bin");
        assert_eq!(expand("/usr/bin", "/home/user"), "/usr/bin");
    }

    #[test]
    fn key_normalizes_the_entries() {
        assert_eq!(key("/usr/bin/", "/home/user"), "/usr/bin");
        assert_eq!(key("/usr//bin", "/home/user"), "/usr/bin");
        assert_eq!(key("$HOME/.go/bin", "/home/user"), "/home/user/.go/bin");
        assert_eq!(key("~/.go/bin", "/home/user"), "/home/user/.go/bin");
        assert_eq!(key("/", "/home/user"), "/");
    }
}
