use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
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
    pub flush: bool,
    pub widget: Option<ModuleWidgetNode>,
}

#[derive(Debug, Clone)]
pub struct ModuleWidgetNode {
    pub module: String,
    pub kind: String,
    pub title: String,
    pub text: String,
    pub lines: Vec<String>,
    pub value: f64,
    pub max: f64,
    pub direction: String,
    pub flex: u16,
    pub children: Vec<ModuleWidgetNode>,
}

impl ModuleWidgetNode {
    fn from_value(module: &str, value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let kind = object
            .get("kind")
            .or_else(|| object.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("paragraph")
            .to_string();
        let children = object
            .get("children")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| Self::from_value(module, item))
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            module: module.to_string(),
            kind,
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
            direction: object
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("column")
                .to_string(),
            flex: object
                .get("flex")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, u16::MAX as u64) as u16,
            children,
        })
    }
}

impl ModuleOutput {
    fn parse(module: &str, value: Value) -> Option<Self> {
        let object = value.as_object()?;
        let flush = object.get("flush").and_then(Value::as_bool).unwrap_or(true);
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("status");
        let widget_value = object.get("widget").or_else(|| object.get("root"));
        let widget = widget_value
            .and_then(|value| ModuleWidgetNode::from_value(module, value))
            .or_else(|| {
                matches!(kind, "paragraph" | "list" | "gauge")
                    .then(|| ModuleWidgetNode::from_value(module, &value))
                    .flatten()
            });
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
            flush,
            widget,
        })
    }
}

#[derive(Clone)]
pub struct ModuleRuntime {
    sessions: Arc<Mutex<HashMap<String, Sender<Value>>>>,
    output: Sender<ModuleOutput>,
}

impl ModuleRuntime {
    pub fn new(output: Sender<ModuleOutput>) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            output,
        }
    }

    #[cfg(test)]
    pub(crate) fn output_sender(&self) -> Sender<ModuleOutput> {
        self.output.clone()
    }

    pub fn emit(&self, settings: &Settings, event: &str, song: Option<&Song>) {
        for module in settings
            .modules
            .iter()
            .filter(|module| module.enabled && module.rpc && module.session)
        {
            if !module_accepts_event(module, event) {
                continue;
            }
            let _ = self.send_session(module, rpc_request(event, &[], song));
        }
        emit_with_output(settings, event, song, Some(&self.output));
    }

    pub fn dispatch_command(
        &self,
        settings: &Settings,
        command_name: &str,
        args: &[String],
        song: Option<&Song>,
    ) -> Option<Result<String, String>> {
        for module in settings.modules.iter().filter(|module| module.enabled) {
            let Some(alias) = module
                .commands
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(command_name))
            else {
                continue;
            };
            if module.rpc && module.session {
                let mut request_args = alias.args.clone();
                request_args.extend(args.iter().cloned());
                let event = format!("command:{command_name}");
                return Some(
                    self.send_session(module, rpc_request(&event, &request_args, song))
                        .map(|_| format!("module {}: {}", module.name, command_name)),
                );
            }
            break;
        }
        dispatch_command_with_output(settings, command_name, args, song, Some(&self.output))
    }

    pub fn send_input(&self, settings: &Settings, module_name: &str, key: &str) -> bool {
        let Some(module) = settings.modules.iter().find(|module| {
            module.enabled
                && module.rpc
                && module.session
                && module.name.eq_ignore_ascii_case(module_name)
        }) else {
            return false;
        };
        self.send_session(
            module,
            json!({
                "api_version": 1,
                "type": "input",
                "event": "key",
                "key": key
            }),
        )
        .is_ok()
    }

    pub fn reset(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
    }

    fn send_session(
        &self,
        module: &crate::config::ModuleConfig,
        request: Value,
    ) -> Result<(), String> {
        let name = module.name.clone();
        let sender = self
            .sessions
            .lock()
            .map_err(|_| "module session lock poisoned".to_string())?
            .get(&name)
            .cloned();
        if let Some(sender) = sender {
            if sender.send(request.clone()).is_ok() {
                return Ok(());
            }
            self.sessions
                .lock()
                .map_err(|_| "module session lock poisoned".to_string())?
                .remove(&name);
        }
        let sender = spawn_session(module, self.output.clone())
            .map_err(|error| format!("cannot start module {name}: {error}"))?;
        sender
            .send(request)
            .map_err(|_| format!("module session {name} closed"))?;
        self.sessions
            .lock()
            .map_err(|_| "module session lock poisoned".to_string())?
            .insert(name, sender);
        Ok(())
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
            if module.session {
                continue;
            }
            let _ = spawn_rpc(module, module.args.clone(), Vec::new(), event, song, output);
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
            if module.session {
                continue;
            }
            return Some(
                spawn_rpc(
                    module,
                    module.args.clone(),
                    command_args,
                    &event,
                    song,
                    output,
                )
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

fn module_accepts_event(module: &crate::config::ModuleConfig, event: &str) -> bool {
    module.events.is_empty()
        || module
            .events
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(event))
}

fn spawn_session(
    module: &crate::config::ModuleConfig,
    output: Sender<ModuleOutput>,
) -> std::io::Result<Sender<Value>> {
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
    process.args(&module.args);
    process.env("GROOV_API_VERSION", "1");
    process.env("GROOV_EVENT", "session");
    process.env(
        "GROOV_EVENT_JSON",
        "{\"type\":\"session\",\"api_version\":1}",
    );
    for (key, value) in &module.env {
        process.env(key, value);
    }
    process
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = process.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "module stdin unavailable")
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, "module stdout unavailable")
    })?;
    let (request_tx, request_rx) = mpsc::channel::<Value>();
    let module_name = module.name.clone();
    let output_for_reader = output.clone();
    thread::spawn(move || {
        let mut stdin = stdin;
        for request in request_rx {
            if writeln!(stdin, "{}", request)
                .and_then(|_| stdin.flush())
                .is_err()
            {
                break;
            }
        }
    });
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("type").and_then(Value::as_str) == Some("batch") {
                if let Some(messages) = value.get("messages").and_then(Value::as_array) {
                    for (index, message) in messages.iter().enumerate() {
                        let mut message = message.clone();
                        if index + 1 == messages.len()
                            && value.get("flush").and_then(Value::as_bool) == Some(true)
                        {
                            if let Some(object) = message.as_object_mut() {
                                object.insert("flush".into(), Value::Bool(true));
                            }
                        }
                        if let Some(parsed) = ModuleOutput::parse(&module_name, message) {
                            let _ = output_for_reader.send(parsed);
                        }
                    }
                }
            } else if let Some(parsed) = ModuleOutput::parse(&module_name, value) {
                let _ = output_for_reader.send(parsed);
            }
        }
        let _ = child.wait();
    });
    Ok(request_tx)
}

fn spawn_rpc(
    module: &crate::config::ModuleConfig,
    process_args: Vec<String>,
    request_args: Vec<String>,
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
        process_args
            .iter()
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
    let request = rpc_request(event, &request_args, song);
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
                if value.get("type").and_then(Value::as_str) == Some("batch") {
                    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
                        for (index, message) in messages.iter().enumerate() {
                            let mut message = message.clone();
                            if index + 1 == messages.len()
                                && value.get("flush").and_then(Value::as_bool) == Some(true)
                            {
                                if let Some(object) = message.as_object_mut() {
                                    object.insert("flush".into(), Value::Bool(true));
                                }
                            }
                            if let Some(parsed) = ModuleOutput::parse(&module_name, message) {
                                if let Some(output) = &output {
                                    let _ = output.send(parsed);
                                }
                            }
                        }
                    }
                } else if let Some(message) = ModuleOutput::parse(&module_name, value) {
                    if let Some(output) = &output {
                        let _ = output.send(message);
                    }
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
    use super::{spawn_rpc, spawn_session, ModuleOutput, ModuleWidgetNode};
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
            Vec::new(),
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

    #[test]
    fn rpc_session_streams_multiple_requests() {
        let module: ModuleConfig = serde_json::from_str(
            r#"{"name":"rpc-session","rpc":true,"session":true,"command":"sh","args":["-c","while read line; do printf '{\"type\":\"status\",\"text\":\"ok\"}\\n'; done"]}"#,
        )
        .unwrap();
        let (tx, rx) = mpsc::channel::<ModuleOutput>();
        let session = spawn_session(&module, tx).unwrap();
        session.send(serde_json::json!({"event":"one"})).unwrap();
        session.send(serde_json::json!({"event":"two"})).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap().text, "ok");
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap().text, "ok");
    }

    #[test]
    fn widget_tree_parses_nested_layout_and_children() {
        let output = ModuleOutput::parse(
            "tree",
            serde_json::json!({
                "type":"view",
                "root": {
                    "kind":"row",
                    "children":[
                        {"kind":"paragraph","text":"left","flex":2},
                        {"kind":"gauge","value":3,"max":4,"flex":1}
                    ]
                }
            }),
        )
        .unwrap();
        let widget = output.widget.unwrap();
        assert_eq!(widget.kind, "row");
        assert_eq!(widget.children.len(), 2);
        assert_eq!(widget.children[0].flex, 2);
        let _typed: ModuleWidgetNode = widget;
    }
}
