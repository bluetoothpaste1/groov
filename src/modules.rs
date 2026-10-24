use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use crate::config::Settings;
use crate::library::Song;
use serde_json::{json, Value};

/// A JSONL response emitted by an RPC module. The TUI interprets the `type`
/// field and keeps unknown fields available in `data` for future extensions.
#[derive(Debug, Clone)]
pub struct ModuleOutput {
    pub module: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub lines: Vec<String>,
    pub value: f64,
    pub max: f64,
    pub data: Value,
}

impl ModuleOutput {
    fn parse(module: &str, value: Value) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            module: module.to_string(),
            kind: object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("status")
                .to_string(),
            title: object
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: object
                .get("text")
                .or_else(|| object.get("body"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            lines: object
                .get("lines")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            value: object.get("value").and_then(Value::as_f64).unwrap_or(0.0),
            max: object.get("max").and_then(Value::as_f64).unwrap_or(1.0),
            data: value,
        })
    }
}

/// Fire-and-forget hooks for user-owned command modules. Module commands are
/// deliberately external processes so a broken module cannot corrupt the TUI.
pub fn emit_with_output(
    settings: &Settings,
    event: &str,
    song: Option<&Song>,
    output: Option<&Sender<ModuleOutput>>,
) {
    for module in settings.modules.iter().filter(|module| module.enabled) {
        let Some(command) = module.command.as_deref() else {
            continue;
        };
        if !module.events.is_empty()
            && !module
                .events
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(event))
        {
            continue;
        }
        if module.rpc {
            let _ = spawn_rpc(module, module.args.clone(), event, song, output);
            continue;
        }
        let mut process = Command::new(command);
        if let Some(cwd) = module.cwd.as_deref() {
            process.current_dir(cwd);
        }
        process.args(
            module
                .args
                .iter()
                .map(|argument| interpolate(argument, event, song)),
        );
        process.env("GROOV_EVENT", event);
        process.env("GROOV_API_VERSION", "1");
        process.env("GROOV_EVENT_JSON", event_payload(event, song));
        process.stdin(Stdio::null());
        process.stdout(Stdio::null());
        process.stderr(Stdio::null());
        for (key, value) in &module.env {
            process.env(key, interpolate(value, event, song));
        }
        if let Some(song) = song {
            process.env("GROOV_SONG_ID", &song.info.id);
            process.env("GROOV_SONG_FILE", &song.audio);
        }
        let _ = spawn_detached(process);
    }
}

/// Return a module command bound to the first token, if any module exposes it.
/// The caller owns parsing and displaying the result; this boundary keeps
/// third-party code out of the TUI and makes modules language independent.
pub fn dispatch_command_with_output(
    settings: &Settings,
    command_name: &str,
    args: &[String],
    song: Option<&Song>,
    output: Option<&Sender<ModuleOutput>>,
) -> Option<Result<String, String>> {
    for module in settings.modules.iter().filter(|module| module.enabled) {
        let Some(command) = module.command.as_deref() else {
            continue;
        };
        let Some(alias) = module
            .commands
            .iter()
            .find(|item| item.name.eq_ignore_ascii_case(command_name))
        else {
            continue;
        };
        let mut process = Command::new(command);
        if let Some(cwd) = module.cwd.as_deref() {
            process.current_dir(cwd);
        }
        let event = format!("command:{command_name}");
        let mut command_args = alias.args.clone();
        command_args.extend(args.iter().cloned());
        if module.rpc {
            return Some(
                spawn_rpc(module, command_args, &event, song, output)
                    .map(|_| format!("module {}: {}", module.name, command_name))
                    .map_err(|error| format!("cannot start module {}: {error}", module.name)),
            );
        }
        process.args(
            command_args
                .iter()
                .map(|value| interpolate(value, &event, song)),
        );
        process.env("GROOV_EVENT", &event);
        process.env("GROOV_API_VERSION", "1");
        process.env("GROOV_EVENT_JSON", event_payload(&event, song));
        process.stdin(Stdio::null());
        process.stdout(Stdio::null());
        process.stderr(Stdio::null());
        for (key, value) in &module.env {
            process.env(key, interpolate(value, &event, song));
        }
        if let Some(song) = song {
            process.env("GROOV_SONG_ID", &song.info.id);
            process.env("GROOV_SONG_FILE", &song.audio);
        }
        return Some(
            spawn_detached(process)
                .map(|_| format!("module {}: {}", module.name, command_name))
                .map_err(|error| format!("cannot start module {}: {error}", module.name)),
        );
    }
    None
}

fn spawn_rpc(
    module: &crate::config::ModuleConfig,
    args: Vec<String>,
    event: &str,
    song: Option<&Song>,
    output: Option<&Sender<ModuleOutput>>,
) -> std::io::Result<()> {
    let command = module.command.as_deref().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "module command is missing",
        )
    })?;
    let mut process = Command::new(command);
    if let Some(cwd) = module.cwd.as_deref() {
        process.current_dir(cwd);
    }
    process.args(
        args.iter()
            .map(|argument| interpolate(argument, event, song)),
    );
    process.env("GROOV_API_VERSION", "1");
    process.env("GROOV_EVENT", event);
    process.env("GROOV_EVENT_JSON", event_payload(event, song));
    for (key, value) in &module.env {
        process.env(key, interpolate(value, event, song));
    }
    if let Some(song) = song {
        process.env("GROOV_SONG_ID", &song.info.id);
        process.env("GROOV_SONG_FILE", &song.audio);
    }
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = process.spawn()?;
    let request = rpc_request(event, &args, song);
    let module_name = module.name.clone();
    let output = output.cloned();
    thread::spawn(move || {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{}", request);
        }
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let Some(message) = ModuleOutput::parse(&module_name, value) else {
                    continue;
                };
                if let Some(output) = &output {
                    let _ = output.send(message);
                }
            }
        }
        let _ = child.wait();
    });
    Ok(())
}

fn spawn_detached(mut process: Command) -> std::io::Result<()> {
    let child = process.spawn()?;
    thread::spawn(move || wait_for_module(child));
    Ok(())
}

fn wait_for_module(mut child: Child) {
    let _ = child.wait();
}

pub fn key_action(settings: &Settings, key: &str) -> Option<String> {
    settings
        .modules
        .iter()
        .filter(|module| module.enabled)
        .find_map(|module| {
            module
                .keys
                .iter()
                .find_map(|(configured, action)| (configured == key).then(|| action.clone()))
        })
}

fn event_payload(event: &str, song: Option<&Song>) -> String {
    let mut payload = BTreeMap::from([(String::from("event"), event.to_string())]);
    if let Some(song) = song {
        payload.insert("id".into(), song.info.id.clone());
        payload.insert("file".into(), song.audio.to_string_lossy().into_owned());
        payload.insert("title".into(), song.info.title.clone());
        payload.insert("artist".into(), song.info.artist.clone());
    }
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into())
}

fn rpc_request(event: &str, args: &[String], song: Option<&Song>) -> Value {
    json!({
        "api_version": 1,
        "type": if event.starts_with("command:") { "command" } else { "event" },
        "event": event,
        "args": args,
        "song": song.map(|song| json!({
            "id": song.info.id,
            "file": song.audio.to_string_lossy(),
            "title": song.info.title,
            "artist": song.info.artist,
            "album": song.info.album,
            "description": song.info.description,
            "tags": song.info.tags,
            "duration": song.info.duration_seconds
        }))
    })
}

fn interpolate(value: &str, event: &str, song: Option<&Song>) -> String {
    let value = value.replace("{event}", event);
    match song {
        Some(song) => value
            .replace("{id}", &song.info.id)
            .replace("{file}", &song.audio.to_string_lossy()),
        None => value,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{spawn_rpc, ModuleOutput};
    use crate::config::ModuleConfig;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn rpc_process_receives_json_and_returns_a_view() {
        let module: ModuleConfig =
            serde_json::from_str(r#"{"name":"rpc-test","rpc":true,"command":"sh"}"#).unwrap();
        let (tx, rx) = mpsc::channel::<ModuleOutput>();
        spawn_rpc(
            &module,
            vec![
                "-c".into(),
                "read request; printf '{\"type\":\"status\",\"text\":\"ok\"}\\n'".into(),
            ],
            "command:test",
            None,
            Some(&tx),
        )
        .unwrap();
        let output = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(output.module, "rpc-test");
        assert_eq!(output.kind, "status");
        assert_eq!(output.text, "ok");
    }
}
