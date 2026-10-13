---
name: chpath
description: Manage the Unix PATH environment variable with the chpath CLI. Use when adding, removing, listing, checking, or cleaning up directories in PATH, including dead directories and duplicated entries, by rewriting the Bash configuration file (~/.bashrc).
license: BSD-3-Clause
---

# chpath

`chpath` manages the Unix `PATH` environment variable by rewriting the line that sets `PATH` in the user's shell configuration file, so changes persist across new shell sessions. Prefer it over hand-editing `~/.bashrc`: it handles path normalization, idempotent edits, and atomic writes.

## Requirements and limits

- Only **bash** is supported for now. The shell is read from `$SHELL` and must be `bash`; the configuration file is `$HOME/.bashrc`. Any other shell ends with an explicit error. `$HOME` must be set.
- `chpath` locates the single line that sets `PATH` in `~/.bashrc`: `PATH=...` or `export PATH=...`, optionally quoted and with a trailing inline comment. It fails when no such line exists.
- Permission and atomicity guarantees: `--write` preserves the file's permissions, creates a brand-new file with mode `600`, and replaces the file atomically through a temporary file, so an interrupted write never truncates the configuration.

## Commands

| Command | Effect |
| --- | --- |
| `chpath --list` / `-l` | Print every current `PATH` entry, one per line. |
| `chpath --check` / `-c` | Print dead and duplicated entries (or `PATH is OK`). Read-only, always exits `0`. |
| `chpath --add <PATH>` / `-a` | Append a directory to `PATH` (idempotent). |
| `chpath --remove <PATH>` / `-r` | Remove a directory from `PATH`; errors if absent. |
| `chpath --cleanup` / `-u` | Drop dead and duplicated entries. |
| `chpath --write` / `-w` | Rewrite the configuration file in place instead of printing it to stdout. |
| `chpath --backup` / `-b` | With `--write`, save `<config>.bak` first. |
| `chpath --version` / `-v` | Print the version with the current git short hash. |

The operations `--add`, `--remove`, `--check`, `--cleanup` and `--list` are mutually exclusive. Without any flag or argument, `chpath` prints its help.

## Safe workflow

1. By default nothing is modified: `--add`, `--remove` and `--cleanup` print the whole rewritten configuration file to stdout. Preview and inspect it before persisting.
2. To persist, add `--write`, optionally with `--backup`: `chpath --add ~/.go/bin --write --backup`.
3. Audit first, then fix: run `chpath --check`, then `chpath --cleanup --write`. `--check` and `--cleanup` emit exactly the same lines, in `PATH` order; `--check` changes nothing and always exits `0`.
4. Flag rules: `--write` cannot be combined with `--check` or `--list`, and must accompany `--add`, `--remove` or `--cleanup`. `--backup` requires `--write`.
5. Exit code is `0` on success and `1` on error; error messages go to stderr prefixed with `chpath:`. Confirmation messages (added, removed) also go to stderr.

## Path normalization

Entries are compared in normalized form:

- The prefixes `$HOME`, `${HOME}` and `~` are expanded.
- Repeated slashes are collapsed and a trailing slash is dropped, so `/usr/bin/`, `/usr//bin` and `/usr/bin` are equal.
- Therefore `--add` is idempotent: adding a path already present (in any normalized form) leaves `PATH` unchanged. Adding a path that is already in the live `PATH`, when the configuration file contains the `$PATH` pseudo-entry, is also a no-op.
- In `--list`, the pseudo-entry `$PATH` is expanded in place with the live `PATH` value, empty entries are skipped, and duplicates are dropped keeping the first occurrence. Explicit entries are printed verbatim, as stored in the configuration file.
- A "dead" path is one whose directory no longer exists. `--check` and `--cleanup` skip `$PATH`, empty entries, and entries holding an environment variable reference `chpath` cannot expand (for example `$XDG_BIN_HOME`, `${FOO}`).

## Examples

```console
$ chpath --list
$ chpath --check
$ chpath --add $HOME/.go/bin            # preview the rewritten ~/.bashrc on stdout
$ chpath --add $HOME/.go/bin --write    # persist the change in ~/.bashrc
$ chpath --cleanup --write --backup     # drop dead/duplicate entries, keep a .bak
$ chpath --remove /usr/lib/jvm/default/bin --write
```

To add a literal `$HOME` (kept unexpanded), quote it:
`chpath --add '$HOME/.go/bin' --write`.
