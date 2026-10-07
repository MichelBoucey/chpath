# chpath, a CLI tool to manage the modification of the Unix environment variable `PATH` [![CI](https://github.com/MichelBoucey/chpath/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/MichelBoucey/chpath/actions/workflows/ci.yml)

## 1. Goal

For the record, the `PATH` variable holds a list of directories to search for executable programs, in order. When a command is typed without a full path, the system checks those directories and uses the first matching executable it finds.

Over time `PATH` tends to degrade: the same directory is added twice or more, and directories that no longer exist (uninstalled tools, moved SDKs) keep slowing down every command lookup. The goal of `chpath` is to manage the modification of `PATH` carefully:

- add and remove directory paths,
- check for directory paths that no longer exist in the file system,
- cleanup those dead directory paths,
- list the current directory paths.

`chpath` rewrites the shell configuration file where `PATH` is set instead of patching the environment of the current process only, so the changes persist across new shell sessions.

*N.B.*: for now, `chpath` is just implemented for **Bash**.

## 2. Security 

`chpath` is created to modify the shell configuration files like `.bashrc` that should include only trivial exports and aliases, not API tokens or other secrets. At least the permissions on user's shell configuration files should be `600`.

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
chpath [OPTIONS]
```

Without flags or arguments, `chpath` outputs the help as the `--help` do.

the used shell is read from the environment variable `SHELL` and the changes are written in its configuration file, `$HOME/.bashrc`. Any other shell ends with an explicit error.

## 5. Flags

### 5.1. `--add`, `-a`

Add the given path in argument to `PATH`.

```console
$ chpath --add $HOME/.go/bin
```

The path is appended to the value of `PATH`; a confirmation message is emitted to tell that the new path has been added to `PATH`. Adding a path already present in `PATH` leaves `PATH` unchanged (`--add` is idempotent).

*N.B.*: if you want to add `$HOME` literally to your `PATH`, quote the path to add like this, `chpath --add '$HOME/.go/bin'`.

### 5.2. `--remove`, `-r`

Remove the given path in argument from `PATH`.

```console
$ chpath --remove /usr/lib/jvm/default/bin
```

A confirmation message is emitted when the path is removed. If the given path doesn't exist in `PATH`, `chpath` exits in error with an error message.

### 5.3. `--check`, `-c`

Check if some paths already added to `PATH` no more exist in the file system.

```console
$ chpath --check
```

`chpath` outputs the dead directory paths present in the `PATH` value, one per output line. Nothing is modified and the exit code stays `0`, even when dead paths are found.

### 5.4. `--cleanup`, `-u`

Remove the paths from `PATH` that have no directory paths corresponding in the file system.

```console
$ chpath --cleanup
```

The dead directory paths removed from the `PATH` value are output, one per output line, and the re-written configuration file follows the default output behavior (see `--write`).

### 5.5. `--list`, `-l`

Output the current paths of `PATH`, one per line.

```console
$ chpath --list
```

The paths are output verbatim, as stored in the configuration file. The pseudo-entry `$PATH`, which concatenates the old value of `PATH`, and the empty entries are skipped.

### 5.6. `--write`, `-w`

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

### 5.7. `--version`, `-v`

Output the version of `chpath` with the current git short hash:

```console
$ chpath --version
chpath 0.1.0 (33c0e12)
```

### 5.8. `--help`, `-h`

Output the help of `chpath`.

## 6. Examples

Given the following line in `$HOME/.bashrc`:

```sh
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin:$HOME/.old/tools
```

with `$HOME/.old/tools` being a directory which no longer exists.

List the current paths of `PATH`:

```console
$ chpath --list
/usr/local/sbin
/usr/local/bin
/usr/bin
$HOME/.local/bin
/usr/lib/jvm/default/bin
$HOME/.old/tools
```

Add the directory of the Go binaries and output the re-written configuration file:

```console
$ chpath --add $HOME/.go/bin
the path '/home/user/.go/bin' has been added to PATH
```

Which gives the PATH's value:

```
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin:$HOME/.old/tools:/home/user/.go/bin
```

Find the dead directory paths:

```console
$ chpath --check
$HOME/.old/tools
```

Remove the dead directory paths and rewrite the configuration file:

```console
$ chpath --cleanup --write
$HOME/.old/tools
```

`$HOME/.bashrc` now reads:

```sh
export PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:$PATH:$HOME/.local/bin:/usr/lib/jvm/default/bin
```

## 7. License

The CLI tool `chpath` is released under 3-Clause BSD License.

