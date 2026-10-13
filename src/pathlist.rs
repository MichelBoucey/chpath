use std::collections::HashSet;
use std::path::Path;

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
/// the path is not present in PATH.
pub fn remove(entries: &[String], path: &str, home: &str) -> Result<Vec<String>, String> {
    let target = key(path, home);

    if !entries.iter().any(|entry| key(entry, home) == target) {
        return Err(format!("the path '{path}' is not present in PATH"));
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

/// Output the PATH entries which are real paths, skipping the pseudo-entry
/// `$PATH` and the empty entries.
pub fn visible_paths(entries: &[String]) -> Vec<&String> {
    entries
        .iter()
        .filter(|entry| !entry.is_empty() && entry.as_str() != "$PATH")
        .collect()
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

        assert!(remove(&entries, "/opt/bin", "/home/user").is_err());
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
    fn visible_paths_skips_the_path_item_and_empty_entries() {
        let entries = parse("/usr/bin:$PATH::/opt/bin");

        let visible = visible_paths(&entries);

        assert_eq!(visible, vec!["/usr/bin", "/opt/bin"]);
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
