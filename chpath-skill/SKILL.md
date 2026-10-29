---
name: chpath
description: Manage the Unix PATH environment variable with the chpath CLI. Use when adding, removing, listing, checking, or cleaning up directories in PATH, including dead directories and duplicated entries, by rewriting the Bash configuration file (~/.bashrc).
license: BSD-3-Clause
---

# chpath

`chpath` manages the Unix `PATH` environment variable by rewriting the line that sets `PATH` in the user's shell configuration file, so changes persist across new shell sessions. Prefer it over hand-editing `~/.bashrc`: it handles path normalization, idempotent edits, and atomic writes.

## Requirements and limits

- Only **bash** is supported for now. The shell is read from `$SHELL` and must be `bash`; the configuration file is `$HOME/.bashrc`. Any other shell ends with an explicit error. `$HOME` must be set.
- `chpath` locates the single line that sets `PATH` in `~/.bashrc`: `PATH=...`, `export PATH=...`, `declare -x PATH=...` or `typeset -x PATH=...`, optionally indented, quoted and with a trailing inline comment. It fails when no such line exists, when several lines set `PATH` (the line numbers are listed in the error), when the line continues on the next line with `\`, and when it appends with `PATH+=`.
- Permission and atomicity guarantees: `--write` preserves the file's permissions, creates a brand-new file with mode `600`, and replaces the file atomically through a temporary file, so an interrupted write never truncates the configuration.
- `--add` refuses, with an explicit error and without touching the file, a path which is not absolute (or not prefixed with `$HOME`, `${HOME}` or `~`), a path which does not name an existing directory (unless `--force` is given), and a path holding a character that cannot be stored safely in the shell configuration file (space, quote, backslash, `#`, `;`, newline...). Only `$NAME` and `${NAME}` references are accepted.
- `--remove` refuses a path which is only inherited from the live `PATH` through the pseudo-entry `$PATH`, because it is not set in the configuration file.

## Commands

| Command | Effect |
| --- | --- |
| `chpath --list` / `-l` | Print every current `PATH` entry, one per line. |
| `chpath --check` / `-c` | Print dead and duplicated entries (or `PATH is OK`). Read-only, always exits `0`. |
| `chpath --add <PATH>` / `-a` | Append a directory to `PATH` (idempotent, validated). |
| `chpath --remove <PATH>` / `-r` | Remove a directory from `PATH`; errors if absent or inherited. |
| `chpath --cleanup` / `-u` | Drop dead and duplicated entries. |
| `chpath --force` / `-f` | With `--add`, allow a directory that does not exist yet. |
| `chpath --write` / `-w` | Rewrite the configuration file in place instead of printing it to stdout. |
| `chpath --backup` / `-b` | With `--write`, save `<config>.bak` first. |
| `chpath --version` / `-v` | Print the version with the current git short hash. |

The operations `--add`, `--remove`, `--check`, `--cleanup` and `--list` are mutually exclusive. Without any flag or argument, `chpath` prints its help.

## Safe workflow

1. By default nothing is modified: `--add`, `--remove` and `--cleanup` print the whole rewritten configuration file to stdout. Preview and inspect it before persisting.
2. To persist, add `--write`, optionally with `--backup`: `chpath --add ~/.go/bin --write --backup`.
3. Audit first, then fix: run `chpath --check`, then `chpath --cleanup --write`. `--check` and `--cleanup` emit exactly the same lines, the dead paths first and then the duplicated paths; `--check` changes nothing and always exits `0`.
4. Flag rules: `--write` cannot be combined with `--check` or `--list`, and must accompany `--add`, `--remove` or `--cleanup`. `--backup` requires `--write`. `--force` requires `--add`.
5. Exit code is `0` on success and `1` on error; error messages go to stderr prefixed with `chpath:`. Confirmation messages (added, removed) also go to stderr.
6. A refused operation (relative path, unsafe character, missing directory, ambiguous configuration file) never modifies the file, so retrying after fixing the input is safe.

## Path normalization

Entries are compared in normalized form:

- The prefixes `$HOME`, `${HOME}` and `~` are expanded.
- Repeated slashes are collapsed and a trailing slash is dropped, so `/usr/bin/`, `/usr//bin` and `/usr/bin` are equal.
- Therefore `--add` is idempotent: adding a path already present (in any normalized form) leaves `PATH` unchanged. Adding a path that is already in the live `PATH`, when the configuration file contains the `$PATH` pseudo-entry, is also a no-op.
- In `--list`, the pseudo-entry `$PATH` is expanded in place with the live `PATH` value, empty entries are skipped, and duplicates are dropped keeping the first occurrence. Explicit entries are printed verbatim, as stored in the configuration file.
- A "dead" path is one whose directory no longer exists. `--check` and `--cleanup` skip `$PATH`, empty entries, and entries holding an environment variable reference `chpath` cannot expand (for example `$XDG_BIN_HOME`, `${FOO}`).
- `--check` and `--cleanup` print the dead paths first, then the duplicated paths, and always the exact same lines.
- Entries are only appended as written: chpath never quotes them, hence the refusal of spaces and other special characters described above.

## Examples

```console
$ chpath --list
$ chpath --check
$ chpath --add $HOME/.go/bin            # preview the rewritten ~/.bashrc on stdout
$ chpath --add $HOME/.go/bin --write    # persist the change in ~/.bashrc
$ chpath --add /opt/toolchain/bin --force   # the directory is created later
$ chpath --cleanup --write --backup     # drop dead/duplicate entries, keep a .bak
$ chpath --remove /usr/lib/jvm/default/bin --write
```

To add a literal `$HOME` (kept unexpanded), quote it:
`chpath --add '$HOME/.go/bin' --write`.
