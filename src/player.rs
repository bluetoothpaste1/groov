use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::config::Settings;

pub fn start(settings: &Settings, audio: &Path, start_seconds: u64) -> Result<Child, String> {
    start_with_filter_and_control(settings, audio, start_seconds, None, None)
}

/// Start the configured audio backend, optionally applying an audio filter.
/// `{audio_filter}` can be used in a custom `player_command`; for the default
/// ffplay backend the filter is appended automatically.
pub fn start_with_filter(
    settings: &Settings,
    audio: &Path,
    start_seconds: u64,
    audio_filter: Option<&str>,
) -> Result<Child, String> {
    start_with_filter_and_control(settings, audio, start_seconds, audio_filter, None)
}

/// Start the backend with optional audio filtering and a generic live-control
/// endpoint. Backends such as mpv can use `{player_ipc}` to expose their own
/// command protocol without coupling groov to a particular player.
pub fn start_with_filter_and_control(
    settings: &Settings,
    audio: &Path,
    start_seconds: u64,
    audio_filter: Option<&str>,
    player_ipc: Option<&Path>,
) -> Result<Child, String> {
    let quoted = shell_quote(&audio.to_string_lossy());
    let start = start_seconds.to_string();
    let mut player_command = settings.player_command.clone();
    if start_seconds > 0 && !player_command.contains("{start}") && player_command.contains("ffplay")
    {
        player_command = player_command.replacen("ffplay", &format!("ffplay -ss {start}"), 1);
    }
    let filter_placeholder = player_command.contains("{audio_filter}");
    if let Some(filter) = audio_filter.filter(|_| !filter_placeholder) {
        if !player_command.contains("ffplay") {
            return Err("player_command must contain {audio_filter} to use an audio filter".into());
        }
        let filter_option = format!("-af {}", shell_quote(filter));
        if player_command.contains("{file}") {
            player_command =
                player_command.replacen("{file}", &format!("{filter_option} {{file}}"), 1);
        } else {
            player_command.push(' ');
            player_command.push_str(&filter_option);
        }
    }
    let filter_argument = audio_filter
        .map(|filter| format!("-af {}", shell_quote(filter)))
        .unwrap_or_default();
    let player_ipc_argument = player_ipc.map(|path| shell_quote(&path.to_string_lossy()));
    if let Some(path) = player_ipc {
        if player_command.contains("{player_ipc}") {
            let _ = std::fs::remove_file(path);
        }
    }
    let command = player_command
        .replace("{start}", &start)
        .replace("{audio_filter}", &filter_argument)
        .replace("{player_ipc}", player_ipc_argument.as_deref().unwrap_or(""))
        .replace("{file}", &quoted);
    let command = if player_command.contains("{file}") {
        command
    } else {
        format!("{command} {quoted}")
    };
    let command = format!("exec {command}");
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| {
        if cfg!(windows) {
            std::ffi::OsString::from("cmd.exe")
        } else {
            std::ffi::OsString::from("/bin/sh")
        }
    });
    let mut process = Command::new(shell);
    if cfg!(windows) {
        process.args(["/C", &command]);
    } else {
        process.args(["-c", &command]);
    }
    // Audio backends must never write progress bars or warnings over the TUI.
    // Users who need diagnostics can put redirection in `player_command`.
    process
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    process
        .spawn()
        .map_err(|error| format!("cannot start player: {error}"))
}

pub fn stop(child: &mut Option<Child>) {
    if let Some(process) = child.as_mut() {
        let _ = process.kill();
        let _ = process.wait();
    }
    *child = None;
}

pub fn pause(child: &Child) -> Result<(), String> {
    send_signal(child, signal_pause())
}

pub fn resume(child: &Child) -> Result<(), String> {
    send_signal(child, signal_continue())
}

#[cfg(unix)]
fn send_signal(child: &Child, signal: i32) -> Result<(), String> {
    let pid = child.id() as libc::pid_t;
    if unsafe { libc::kill(pid, signal) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "cannot control player process {pid}: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(not(unix))]
fn send_signal(_child: &Child, _signal: i32) -> Result<(), String> {
    Err("pause/resume is unavailable on this platform".into())
}

#[cfg(unix)]
fn signal_pause() -> i32 {
    libc::SIGSTOP
}

#[cfg(not(unix))]
fn signal_pause() -> i32 {
    0
}

#[cfg(unix)]
fn signal_continue() -> i32 {
    libc::SIGCONT
}

#[cfg(not(unix))]
fn signal_continue() -> i32 {
    0
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::{start, start_with_filter, start_with_filter_and_control};
    use crate::config::Settings;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn starts_a_backend_with_quoted_file_and_start_placeholders() {
        let temp = tempdir().unwrap();
        let audio = temp.path().join("track with spaces.mp3");
        fs::write(&audio, b"test").unwrap();
        let settings = Settings {
            player_command: "test \"{start}\" = 7 && test -f {file}".into(),
            ..Settings::default()
        };
        let mut child = start(&settings, &audio, 7).unwrap();
        let status = child.wait().unwrap();
        assert!(status.success());
    }

    #[test]
    fn custom_backend_can_receive_the_filter_placeholder() {
        let temp = tempdir().unwrap();
        let audio = temp.path().join("track.mp3");
        fs::write(&audio, b"test").unwrap();
        let marker = temp.path().join("filter.txt");
        let settings = Settings {
            player_command: format!(
                "printf '%s' \"{{audio_filter}}\" > {} && test -f {{file}}",
                marker.display()
            ),
            ..Settings::default()
        };
        let mut child = start_with_filter(&settings, &audio, 0, Some("atempo=0.750000")).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(fs::read_to_string(marker).unwrap(), "-af 'atempo=0.750000'");
    }

    #[test]
    fn custom_backend_can_receive_the_live_control_endpoint() {
        let temp = tempdir().unwrap();
        let audio = temp.path().join("track.mp3");
        fs::write(&audio, b"test").unwrap();
        let marker = temp.path().join("ipc.txt");
        let ipc = temp.path().join("player.sock");
        let settings = Settings {
            player_command: format!(
                "printf '%s' {{player_ipc}} > {} && test -f {{file}}",
                marker.display()
            ),
            ..Settings::default()
        };
        let mut child =
            start_with_filter_and_control(&settings, &audio, 0, None, Some(&ipc)).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(fs::read_to_string(marker).unwrap(), ipc.to_string_lossy());
    }
}
