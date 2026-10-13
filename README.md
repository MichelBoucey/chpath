# chpath, a CLI tool to manage the modification of the Unix environment variable `PATH` [![CI](https://github.com/MichelBoucey/chpath/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/MichelBoucey/chpath/actions/workflows/ci.yml)

## 1. Goal

For the record, the `PATH` variable holds a list of directories to search for executable programs, in order. When a command is typed without a full path, the system checks those directories and uses the first matching executable it finds.

Over time `PATH` tends to degrade: the same directory is added twice, and directories that no longer exist (uninstalled tools, moved SDKs) keep slowing down every command lookup. The goal of `chpath` (named following `chmod` or `chown`) is to manage the modification of `PATH` carefully:

- add and remove directory paths,
- check for directory paths that no longer exist in the file system and for duplicated directory paths,
- cleanup those dead directory paths and those duplicated directory paths,
- list the current directory paths.

`chpath` rewrites the shell configuration file where `PATH` is set instead of patching the environment of the current process only, so the changes persist across new shell sessions.

*N.B.*: for now, `chpath` is just implemented for **Bash**.

## 2. Security 

`chpath` is created to modify the shell configuration files like `.bashrc` that should include only trivial exports and aliases, not API tokens or other secrets. At least the permissions on user's shell configuration files should be `600`. When `--write` rewrites the file, `chpath` preserves its permissions (a brand new file is created with the mode `600`) and replaces it atomically, so an interrupted write can never leave a truncated configuration file.

## 3. Installation

Requirements:

- a Rust toolchain to build the project
- `git` at build time to embed the current short hash in the version (without it, the version reports `unknown`)

### 3.1. From crates.io

```
cargo install chpath
```

### 3.2. From source

```console
$ cargo build --release
```

The binary is then available at `target/release/chpath`; copy it to a directory of your `PATH` (_of course, isn't it?_), for example:

```console
$ cp target/release/chpath ~/.local/bin/
```

## 4. Usage

```text
[user@box ~] $ chpath
Manage the Unix environment variable PATH ($PATH)

Usage: chpath [OPTIONS]

Options:
  -a, --add <ADD>        Add the given path to PATH
  -r, --remove <REMOVE>  Remove the given path from PATH
  -c, --check            Output the dead and duplicated paths of PATH
  -u, --cleanup          Remove the dead paths and the duplicate paths from PATH
  -l, --list             Output all the current paths of PATH, one per line
  -w, --write            Rewrite the shell configuration file instead of printing it to stdout
  -b, --backup           Write a backup of the shell configuration file before rewriting it
  -v, --version          Output the version of chpath with the current git short hash
  -h, --help             Print help
```

Without flags or arguments, `chpath` outputs the help as the `--help` do.

the used shell is read from the environment variable `SHELL` and the changes are written in its configuration file, `$HOME/.bashrc`. Any other shell ends with an explicit error.

### 4.1. Flags in details

#### 4.1.1. `--add`, `-a`

Add the given path in argument to `PATH`.

```console
$ chpath --add $HOME/.go/bin
```

The path is appended to the value of `PATH`; a confirmation message is emitted to tell that the new path has been added to `PATH`. Adding a path already present in `PATH` leaves `PATH` unchanged (`--add` is idempotent). The paths are compared in their normalized form, so `/usr/bin/` is the same path as `/usr/bin` and `~/.go/bin` is the same path as `$HOME/.go/bin`.

*N.B.*: if you want to add `$HOME` literally to your `PATH`, quote the path to add like this, `chpath --add '$HOME/.go/bin'`.

#### 4.1.2. `--remove`, `-r`

Remove the given path in argument from `PATH`.

```console
$ chpath --remove /usr/lib/jvm/default/bin
```

A confirmation message is emitted when the path is removed. If the given path doesn't exist in `PATH`, `chpath` exits in error with an error message. As for `--add`, the paths are compared in their normalized form, so `/usr/bin/` matches `/usr/bin`.

#### 4.1.3. `--check`, `-c`

Check for the dead directory paths and the duplicated paths present in `PATH`.

```console
$ chpath --check
the directory path '$HOME/.old/tools' no longer exists
the path '/usr/bin' is duplicated in PATH
```

A dead directory path is a path whose directory no more exists in the file system. A duplicated path is a path present more than once in `PATH` (the paths are compared in their normalized form). The issues are output one per output line, in the order of the `PATH` value, and `--cleanup` outputs **exactly the same lines**. When no issue is found, `chpath` outputs `PATH is OK`. Nothing is modified and the exit code stays `0`, even when issues are found.

#### 4.1.4. `--cleanup`, `-u`

Remove the dead and the duplicated paths from `PATH`.

```console
$ chpath --cleanup
the directory path '$HOME/.old/tools' no longer exists
the path '/usr/bin' is duplicated in PATH
```

The issues are output exactly as `--check` does (`PATH is OK` when there is none), then the re-written configuration file follows the default output behavior (see `--write`).

#### 4.1.5. `--list`, `-l`

Output all the current paths of `PATH`, one per line.

```console
$ chpath --list
```

The explicit paths are output verbatim, as stored in the configuration file. The pseudo-entry `$PATH`, which stands for the old value of `PATH`, is expanded in place with the live value of the environment variable `PATH`, so every path is listed. The empty entries are skipped and the duplicated paths are dropped keeping the first occurrence; as everywhere else in `chpath`, the paths are compared in their normalized form, so `$HOME/.local/bin` and `/home/user/.local/bin` are the same path.

#### 4.1.6. `--write`, `-w`

In combination with `--add`, `--remove` and `--cleanup`, rewrite the shell configuration file on disk.

By default, these three flags output the whole re-written shell configuration file to `stdout`, which can be redirected or just inspected:

```console
$ chpath --add $HOME/.go/bin
the path '/home/user/.go/bin' has been added to PATH
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin
```

With `--write`, the configuration file is rewritten in place instead:

```console
$ chpath --add $HOME/.go/bin --write
```

The flag `--write` cannot be combined with `--check` nor `--list`.

The rewrite is atomic: the new content is written to a temporary file in the same directory and renamed over the configuration file, so an interrupted write never leaves a truncated file. The permissions of the file are preserved and a brand new file is created with the mode `600`.

#### 4.1.7. `--backup`, `-b`

In combination with `--write`, write a backup of the shell configuration file to `<configuration file>.bak`, for example `$HOME/.bashrc.bak`, before rewriting it.

```console
$ chpath --add $HOME/.go/bin --write --backup
```

The flag `--backup` cannot be used without `--write`.

#### 4.1.8. `--version`, `-v`

Output the version of `chpath` with the current git short hash:

```console
$ chpath --version
chpath 1.0.0 (f244051) released under 3-Clause BSD License
Copyright © 2026 Michel Boucey (michel.boucey@gmail.com)
```

#### 4.1.9. `--help`, `-h`

Output the help of `chpath`.

## 5. Examples

Given the following line in `$HOME/.bashrc`:

```sh
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin:$HOME/.old/tools:/usr/bin
```

with `$HOME/.old/tools` being a directory which no longer exists and `/usr/bin` being duplicated.

List all the current paths of `PATH` (here with the live `PATH` being `/usr/local/sbin:/usr/local/bin:/usr/bin:/bin`):

```console
$ chpath --list
/usr/local/sbin
/usr/local/bin
/usr/bin
/bin
$HOME/.local/bin
/usr/lib/jvm/default/bin
$HOME/.old/tools
```

The pseudo-entry `$PATH` is expanded in place with the live value of `PATH`, and the duplicated `/usr/bin` is dropped.

Add the directory of the Go binaries and output the re-written configuration file:

```console
$ chpath --add $HOME/.go/bin
the path '/home/user/.go/bin' has been added to PATH
```

Which gives the PATH's value:

```
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin:$HOME/.old/tools:/usr/bin:/home/user/.go/bin
```

Find the dead and the duplicated paths:

```console
$ chpath --check
the directory path '$HOME/.old/tools' no longer exists
the path '/usr/bin' is duplicated in PATH
```

Remove the dead and the duplicated paths and rewrite the configuration file:

```console
$ chpath --cleanup --write
the directory path '$HOME/.old/tools' no longer exists
the path '/usr/bin' is duplicated in PATH
```

`$HOME/.bashrc` now reads:

```sh
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin
```

A new check then reports that `PATH` is clean:

```console
$ chpath --check
PATH is OK
```

## 6. AI agent skill

`chpath` ships an AI agent skill in the [`chpath-skill`](chpath-skill) directory, which teaches a coding agent how to manage `PATH` safely on your behalf.

The skill is made of two files:

- [`chpath-skill/SKILL.md`](chpath-skill/SKILL.md) — the instructions loaded by the agent: the requirements and limits, the reference of the commands, the safe preview, `--write` and `--backup` workflow, the path normalization rules and usage examples,
- [`chpath-skill/LICENSE`](chpath-skill/LICENSE) — its own `3-Clause BSD License`.

Once loaded, the agent can drive `chpath` to list, check, add, remove and clean up the paths of `PATH`, and it always previews the re-written shell configuration file before persisting it with `--write`.

## 7. License

The CLI tool `chpath` is released under `3-Clause BSD License`.

