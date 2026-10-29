use std::env;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway home directory holding a shell configuration file, so that the
/// tests never touch the real one.
struct Sandbox {
    directory: PathBuf,
}

impl Sandbox {
    fn new(contents: &str) -> Self {
        let random = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let directory = env::temp_dir().join(format!(
            "chpath-cli-{}-{random}-{count}",
            std::process::id()
        ));

        fs::create_dir_all(&directory).expect("sandbox is created");
        fs::write(directory.join(".bashrc"), contents).expect("configuration file is written");

        Self { directory }
    }

    fn config(&self) -> PathBuf {
        self.directory.join(".bashrc")
    }

    fn backup(&self) -> PathBuf {
        self.directory.join(".bashrc.bak")
    }

    fn entry(&self, name: &str) -> PathBuf {
        let path = self.directory.join(name);
        fs::create_dir_all(&path).expect("directory is created");

        path
    }

    fn read(&self) -> String {
        fs::read_to_string(self.config()).expect("configuration file is read")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_path(args, "/usr/bin:/bin")
    }

    fn run_with_path(&self, args: &[&str], live_path: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_chpath"))
            .args(args)
            .env("HOME", &self.directory)
            .env("SHELL", "/bin/bash")
            .env("PATH", live_path)
            .output()
            .expect("chpath runs")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn text(path: &Path) -> &str {
    path.to_str().expect("the path is valid UTF-8")
}

#[test]
fn without_arguments_outputs_the_help() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&[]);

    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("Usage: chpath"));
}

#[test]
fn add_previews_the_rewritten_configuration() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools)]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        format!("export PATH=/usr/bin:{}\n", tools.display())
    );
    assert!(stderr(&output).contains("has been added to PATH"));
    assert_eq!(sandbox.read(), "export PATH=/usr/bin\n");
}

#[test]
fn add_is_idempotent() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");

    let first = sandbox.run(&["--add", text(&tools), "--write"]);
    let written = sandbox.read();
    let second = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&first), 0);
    assert!(stderr(&first).contains("has been added to PATH"));
    assert_eq!(code(&second), 0);
    assert!(stderr(&second).contains("is already in PATH"));
    assert_eq!(sandbox.read(), written);
}

#[test]
fn add_keeps_an_already_present_dead_path_unchanged() {
    let sandbox = Sandbox::new("export PATH=/definitely/not/a/directory:/usr/bin\n");

    let output = sandbox.run(&["--add", "/definitely/not/a/directory", "--write"]);

    assert_eq!(code(&output), 0);
    assert!(stderr(&output).contains("is already in PATH"));
    assert_eq!(
        sandbox.read(),
        "export PATH=/definitely/not/a/directory:/usr/bin\n"
    );
}

#[test]
fn add_write_rewrites_the_configuration() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        sandbox.read(),
        format!("export PATH=/usr/bin:{}\n", tools.display())
    );
}

#[test]
fn add_write_creates_a_backup() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools), "--write", "--backup"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        fs::read_to_string(sandbox.backup()).expect("backup is read"),
        "export PATH=/usr/bin\n"
    );
}

#[test]
fn add_write_preserves_the_permissions() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");
    fs::set_permissions(sandbox.config(), fs::Permissions::from_mode(0o640))
        .expect("permissions are set");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    let mode = fs::metadata(sandbox.config())
        .expect("metadata is read")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(code(&output), 0);
    assert_eq!(mode, 0o640);
}

#[test]
fn add_write_keeps_the_symbolic_link() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let tools = sandbox.entry("tools");
    let target = sandbox.directory.join("dotfiles-bashrc");
    fs::rename(sandbox.config(), &target).expect("configuration file is moved");
    symlink(&target, sandbox.config()).expect("symbolic link is created");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&output), 0);
    assert!(
        fs::symlink_metadata(sandbox.config())
            .expect("metadata is read")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&target).expect("target is read"),
        format!("export PATH=/usr/bin:{}\n", tools.display())
    );
}

#[test]
fn add_rejects_a_relative_path() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&["--add", "bin/tools", "--write"]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("is not absolute"));
    assert_eq!(sandbox.read(), "export PATH=/usr/bin\n");
}

#[test]
fn add_rejects_an_unsafe_path() {
    for path in [
        "/opt/my tools/bin",
        "/opt/it's/bin",
        "/opt/back\\slash",
        "/opt/tag#1",
        "/tmp/x\nexport EVIL=1",
    ] {
        let sandbox = Sandbox::new("export PATH=/usr/bin\n");

        let output = sandbox.run(&["--add", path, "--write"]);

        assert_eq!(code(&output), 1, "the path {path:?} is accepted");
        assert!(
            stderr(&output).contains("cannot be written safely"),
            "unexpected error: {}",
            stderr(&output)
        );
        assert_eq!(sandbox.read(), "export PATH=/usr/bin\n");
    }
}

#[test]
fn add_rejects_a_missing_directory() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&["--add", text(sandbox.config().join("gone").as_path())]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("does not exist"));
    assert!(stderr(&output).contains("--force"));
}

#[test]
fn add_force_accepts_a_missing_directory() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");
    let gone = sandbox.config().join("gone");

    let output = sandbox.run(&["--add", text(&gone), "--force", "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        sandbox.read(),
        format!("export PATH=/usr/bin:{}\n", gone.display())
    );
}

#[test]
fn force_requires_add() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&["--force", "--cleanup"]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("--force must be combined with the flag --add"));
}

#[test]
fn add_handles_a_declare_x_line() {
    let sandbox = Sandbox::new("declare -x PATH=\"/usr/bin\"\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        sandbox.read(),
        format!("declare -x PATH=\"/usr/bin:{}\"\n", tools.display())
    );
}

#[test]
fn add_handles_an_indented_export_line() {
    let sandbox = Sandbox::new("if true; then\n  export PATH=/usr/bin\nfi\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        sandbox.read(),
        format!(
            "if true; then\n  export PATH=/usr/bin:{}\nfi\n",
            tools.display()
        )
    );
}

#[test]
fn several_lines_setting_path_are_rejected() {
    let sandbox = Sandbox::new("export PATH=/first\n# comment\nexport PATH=/second\n");

    let output = sandbox.run(&["--check"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("lines 1, 3"),
        "unexpected error: {}",
        stderr(&output)
    );
}

#[test]
fn a_continuation_is_rejected() {
    let sandbox = Sandbox::new("export PATH=/usr/bin:\\\n  /opt/bin\n");

    let output = sandbox.run(&["--list"]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("continues on the next line"));
}

#[test]
fn an_append_is_rejected() {
    let sandbox = Sandbox::new("PATH+=:/opt/bin\n");

    let output = sandbox.run(&["--list"]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("PATH+="));
}

#[test]
fn write_preserves_the_crlf_line_endings() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\r\nexport FOO=bar\r\n");
    let tools = sandbox.entry("tools");

    let output = sandbox.run(&["--add", text(&tools), "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        sandbox.read(),
        format!(
            "export PATH=/usr/bin:{}\r\nexport FOO=bar\r\n",
            tools.display()
        )
    );
}

#[test]
fn check_reports_the_dead_and_the_duplicated_paths() {
    let dead = "/definitely/not/a/directory";
    let sandbox = Sandbox::new(&format!("export PATH=/usr/bin:/usr/bin:{dead}\n"));

    let output = sandbox.run(&["--check"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        format!(
            "the directory path '{dead}' no longer exists\n\
             the path '/usr/bin' is duplicated in PATH\n"
        )
    );
}

#[test]
fn check_reports_a_clean_path() {
    let sandbox = Sandbox::new("export PATH=/usr/bin:/bin:$PATH\n");

    let output = sandbox.run(&["--check"]);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "PATH is OK\n");
}

#[test]
fn cleanup_removes_the_dead_and_the_duplicated_paths() {
    let dead = "/definitely/not/a/directory";
    let sandbox = Sandbox::new(&format!("export PATH=/usr/bin:/usr/bin:{dead}:/bin\n"));

    let output = sandbox.run(&["--cleanup", "--write"]);

    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        format!(
            "the directory path '{dead}' no longer exists\n\
             the path '/usr/bin' is duplicated in PATH\n"
        )
    );
    assert_eq!(sandbox.read(), "export PATH=/usr/bin:/bin\n");
}

#[test]
fn list_expands_the_path_pseudo_entry() {
    let sandbox = Sandbox::new("export PATH=/opt/bin:$PATH:/usr/bin\n");

    let output = sandbox.run_with_path(&["--list"], "/usr/bin:/bin");

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "/opt/bin\n/usr/bin\n/bin\n");
}

#[test]
fn remove_reports_an_inherited_path() {
    let sandbox = Sandbox::new("export PATH=/opt/bin:$PATH\n");

    let output = sandbox.run_with_path(&["--remove", "/bin", "--write"], "/usr/bin:/bin");

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("inherited"),
        "unexpected error: {}",
        stderr(&output)
    );
    assert_eq!(sandbox.read(), "export PATH=/opt/bin:$PATH\n");
}

#[test]
fn remove_drops_the_given_path() {
    let sandbox = Sandbox::new("export PATH=/usr/bin:/opt/bin:/bin\n");

    let output = sandbox.run(&["--remove", "/opt/bin", "--write"]);

    assert_eq!(code(&output), 0);
    assert!(stderr(&output).contains("has been removed from PATH"));
    assert_eq!(sandbox.read(), "export PATH=/usr/bin:/bin\n");
}

#[test]
fn write_conflicts_with_check_and_list() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let with_check = sandbox.run(&["--write", "--check"]);
    let with_list = sandbox.run(&["--write", "--list"]);
    let alone = sandbox.run(&["--write"]);

    assert_eq!(code(&with_check), 1);
    assert!(stderr(&with_check).contains("--write cannot be combined with the flag --check"));
    assert_eq!(code(&with_list), 1);
    assert!(stderr(&with_list).contains("--write cannot be combined with the flag --list"));
    assert_eq!(code(&alone), 1);
    assert!(stderr(&alone).contains("--write must be combined"));
}

#[test]
fn backup_requires_write() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&["--cleanup", "--backup"]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("--backup must be combined with the flag --write"));
}

#[test]
fn version_outputs_the_version() {
    let sandbox = Sandbox::new("export PATH=/usr/bin\n");

    let output = sandbox.run(&["--version"]);

    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("released under 3-Clause BSD License"));
}
