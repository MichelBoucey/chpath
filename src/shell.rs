use std::env;
use std::path::PathBuf;

/// Find the shell configuration file where the environment variable PATH is set.
///
/// The used shell is read from the `SHELL` environment variable. Only `bash`
/// and its configuration file `$HOME/.bashrc` are supported for now.
pub fn config_file() -> Result<PathBuf, String> {
    let shell =
        env::var("SHELL").map_err(|_| "the environment variable SHELL is not set".to_string())?;
    let shell_path = PathBuf::from(&shell);
    let shell_name = shell_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(shell.as_str());

    match shell_name {
        "bash" => Ok(home_dir()?.join(".bashrc")),
        other => Err(format!(
            "the shell '{other}' is not supported, only bash with its configuration file \
             $HOME/.bashrc is supported"
        )),
    }
}

/// Find the home directory of the user from the environment variable `HOME`.
pub fn home_dir() -> Result<PathBuf, String> {
    env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "the environment variable HOME is not set".to_string())
}
