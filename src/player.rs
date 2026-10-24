use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::config::Settings;

pub fn start(settings: &Settings, audio: &Path, start_seconds: u64) -> Result<Child, String> {
    let quoted = shell_quote(&audio.to_string_lossy());
    let start = start_seconds.to_string();
    let mut player_command = settings.player_command.clone();
    if start_seconds > 0 && !player_command.contains("{start}") && player_command.contains("ffplay")
    {
        player_command = player_command.replacen("ffplay", &format!("ffplay -ss {start}"), 1);
    }
    let command = player_command
        .replace("{start}", &start)
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
