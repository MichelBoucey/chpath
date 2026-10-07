use std::path::Path;

/// Append `path` to the given PATH entries unless it is already present.
///
/// Returns the resulting entries and `true` when the path has been added.
/// `--add` is idempotent: adding a path already in PATH leaves PATH unchanged.
pub fn add(entries: &[String], path: &str) -> (Vec<String>, bool) {
    if entries.iter().any(|entry| entry == path) {
        return (entries.to_vec(), false);
    }

    let mut updated = entries.to_vec();
    updated.push(path.to_string());
    (updated, true)
}

/// Remove `path` from the given PATH entries.
///
/// Fails when the path is not present in PATH.
pub fn remove(entries: &[String], path: &str) -> Result<Vec<String>, String> {
    if !entries.iter().any(|entry| entry == path) {
        return Err(format!("the path '{path}' is not present in PATH"));
    }

    Ok(entries
        .iter()
        .filter(|entry| entry.as_str() != path)
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
/// item `$PATH`, which concatenates the old value of PATH, and the empty items
/// are never checked.
pub fn is_dead(entry: &str, home: &str) -> bool {
    if entry.is_empty() || entry == "$PATH" {
        return false;
    }

    !Path::new(&expand(entry, home)).is_dir()
}

/// Keep only the PATH entries whose directory still exists in the file system.
///
/// Returns the kept entries and the removed dead entries.
pub fn cleanup(entries: &[String], home: &str) -> (Vec<String>, Vec<String>) {
    entries
        .iter()
        .cloned()
        .partition(|entry| !is_dead(entry, home))
}

/// Output the PATH entries whose directory no longer exists in the file system.
pub fn dead_paths<'a>(entries: &'a [String], home: &str) -> Vec<&'a String> {
    entries
        .iter()
        .filter(|entry| is_dead(entry, home))
        .collect()
}

/// Output the PATH entries which are real paths, skipping the pseudo-entry
/// `$PATH` and the empty entries.
pub fn visible_paths(entries: &[String]) -> Vec<&String> {
    entries
        .iter()
        .filter(|entry| !entry.is_empty() && entry.as_str() != "$PATH")
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_appends_a_new_path() {
        let entries = parse("/usr/bin:/usr/lib");

        let (updated, added) = add(&entries, "/opt/bin");

        assert!(added);
        assert_eq!(updated, vec!["/usr/bin", "/usr/lib", "/opt/bin"]);
    }

    #[test]
    fn add_is_idempotent() {
        let entries = parse("/usr/bin:/usr/lib");

        let (updated, added) = add(&entries, "/usr/bin");

        assert!(!added);
        assert_eq!(updated, entries);
    }

    #[test]
    fn remove_drops_the_given_path() {
        let entries = parse("/usr/bin:/usr/lib:/opt/bin");

        let updated = remove(&entries, "/usr/lib").expect("path is present");

        assert_eq!(updated, vec!["/usr/bin", "/opt/bin"]);
    }

    #[test]
    fn remove_fails_on_an_absent_path() {
        let entries = parse("/usr/bin");

        assert!(remove(&entries, "/opt/bin").is_err());
    }

    #[test]
    fn cleanup_removes_only_the_dead_paths() {
        let entries = parse("/definitely/not/a/directory:/tmp");

        let (kept, removed) = cleanup(&entries, "/home/user");

        assert_eq!(kept, vec!["/tmp"]);
        assert_eq!(removed, vec!["/definitely/not/a/directory"]);
    }

    #[test]
    fn dead_paths_skips_the_path_item() {
        let entries = parse("$PATH:/definitely/not/a/directory");

        let dead = dead_paths(&entries, "/home/user");

        assert_eq!(dead, vec!["/definitely/not/a/directory"]);
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
}
