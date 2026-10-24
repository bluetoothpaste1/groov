use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::paths::AppPaths;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_refresh_ms")]
    pub refresh_ms: u64,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_image_mode")]
    pub image_mode: String,
    #[serde(default = "default_player_command")]
    pub player_command: String,
    #[serde(default = "default_skip_seconds")]
    pub skip_seconds: u64,
    #[serde(default)]
    pub repeat: bool,
    /// Keep the selected library row aligned with the track that starts playing.
    #[serde(default)]
    pub follow_playback: bool,
    #[serde(default = "default_cover_size")]
    pub cover_size: u16,
    #[serde(default)]
    pub modules: Vec<ModuleConfig>,
    /// Named user themes. Built-in themes are always available; entries here
    /// let users install and share palettes without changing the binary.
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeConfig>,
    #[serde(default)]
    pub keymap: KeymapConfig,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub enabled: bool,
    /// Use the JSONL request/response protocol instead of fire-and-forget env hooks.
    #[serde(default)]
    pub rpc: bool,
    /// Keep an RPC process alive and stream requests over its stdin.
    #[serde(default)]
    pub session: bool,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional command aliases exposed by the module in COMMAND mode.
    #[serde(default)]
    pub commands: Vec<ModuleCommand>,
    /// Optional key bindings. Values are command lines, for example
    /// `{ "v": "visualizer toggle" }`.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// Extra environment variables passed to the module process.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// If present, only these playback events are sent to the module.
    #[serde(default)]
    pub events: Vec<String>,
    /// Working directory for relative module executables and assets.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
    /// Source manifest for discovered modules. This is runtime metadata and
    /// is intentionally not written back into settings.json.
    #[serde(skip)]
    pub manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleCommand {
    pub name: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_foreground", alias = "fg", alias = "text")]
    pub foreground: String,
    #[serde(default = "default_theme_background", alias = "bg")]
    pub background: String,
    #[serde(default = "default_theme_accent", alias = "primary")]
    pub accent: String,
    #[serde(default = "default_theme_muted", alias = "secondary")]
    pub muted: String,
    #[serde(default = "default_theme_highlight", alias = "selection")]
    pub highlight: String,
    #[serde(default = "default_theme_playing", alias = "active")]
    pub playing: String,
    #[serde(default = "default_theme_border", alias = "line")]
    pub border: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            foreground: default_theme_foreground(),
            background: default_theme_background(),
            accent: default_theme_accent(),
            muted: default_theme_muted(),
            highlight: default_theme_highlight(),
            playing: default_theme_playing(),
            border: default_theme_border(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeymapConfig {
    #[serde(default = "default_quit")]
    pub quit: String,
    #[serde(default = "default_play")]
    pub play: String,
    #[serde(default = "default_stop")]
    pub stop: String,
    #[serde(default = "default_search")]
    pub search: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            refresh_ms: 100,
            theme: "monochrome".into(),
            image_mode: "auto".into(),
            player_command: "ffplay -nodisp -autoexit -loglevel quiet {file}".into(),
            skip_seconds: 5,
            repeat: false,
            follow_playback: false,
            cover_size: 16,
            modules: Vec::new(),
            themes: BTreeMap::new(),
            keymap: KeymapConfig::default(),
            extra: serde_json::Map::new(),
        }
    }
}

impl Default for KeymapConfig {
    fn default() -> Self {
        Self {
            quit: "q".into(),
            play: "space".into(),
            stop: "x".into(),
            search: "/".into(),
        }
    }
}

pub fn load(paths: &AppPaths) -> Result<Settings, ConfigError> {
    if !paths.settings_file.exists() {
        let mut settings = Settings::default();
        load_module_manifests(paths, &mut settings)?;
        load_theme_files(paths, &mut settings)?;
        return Ok(settings);
    }
    let path = paths.settings_file.to_string_lossy().into_owned();
    let text = fs::read_to_string(&paths.settings_file).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let mut settings: Settings =
        serde_json::from_str(&text).map_err(|source| ConfigError::Json {
            path: path.clone(),
            source,
        })?;
    load_module_manifests(paths, &mut settings)?;
    load_theme_files(paths, &mut settings)?;
    Ok(settings)
}

pub fn write_default(paths: &AppPaths) -> Result<(), ConfigError> {
    write_settings(paths, &Settings::default())
}

pub fn write_settings(paths: &AppPaths, settings: &Settings) -> Result<(), ConfigError> {
    let json = serde_json::to_string_pretty(settings).expect("settings are serializable");
    let temporary = paths.settings_file.with_extension("json.tmp");
    fs::write(&temporary, format!("{json}\n")).map_err(|source| ConfigError::Write {
        path: temporary.display().to_string(),
        source,
    })?;
    if let Err(source) = fs::rename(&temporary, &paths.settings_file) {
        let _ = fs::remove_file(&temporary);
        return Err(ConfigError::Write {
            path: paths.settings_file.display().to_string(),
            source,
        });
    }
    Ok(())
}

pub fn ensure(paths: &AppPaths) -> Result<Settings, ConfigError> {
    if !paths.settings_file.exists() {
        write_default(paths)?;
    }
    load(paths)
}

/// Load standalone modules from `~/.config/groov/modules/<name>/module.json`.
/// A manifest is intentionally just JSON, so a module can be written in any
/// language that can read environment variables and launch a process.
fn load_module_manifests(paths: &AppPaths, settings: &mut Settings) -> Result<(), ConfigError> {
    let directory = paths.config_dir.join("modules");
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("module.json");
        if !manifest.is_file() {
            continue;
        }
        let text = match fs::read_to_string(&manifest) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let module = match serde_json::from_str::<ModuleConfig>(&text) {
            Ok(module) => module,
            Err(_) => continue,
        };
        let mut module = module;
        if module.cwd.is_none() {
            module.cwd = Some(entry.path().to_string_lossy().into_owned());
        }
        module.manifest_path = Some(manifest.clone());
        if let Some(existing) = settings
            .modules
            .iter_mut()
            .find(|item| item.name.eq_ignore_ascii_case(&module.name))
        {
            // Explicit settings win, while a manifest supplies omitted hooks.
            if existing.command.is_none() {
                existing.command = module.command;
            }
            if !existing.rpc {
                existing.rpc = module.rpc;
            }
            if existing.version.is_empty() {
                existing.version = module.version;
            }
            if existing.description.is_empty() {
                existing.description = module.description;
            }
            if existing.author.is_empty() {
                existing.author = module.author;
            }
            if existing.homepage.is_empty() {
                existing.homepage = module.homepage;
            }
            if existing.args.is_empty() {
                existing.args = module.args;
            }
            if existing.commands.is_empty() {
                existing.commands = module.commands;
            }
            if existing.keys.is_empty() {
                existing.keys = module.keys;
            }
            if existing.env.is_empty() {
                existing.env = module.env;
            }
            if existing.events.is_empty() {
                existing.events = module.events;
            }
            if existing.cwd.is_none() {
                existing.cwd = module.cwd;
            }
            if existing.manifest_path.is_none() {
                existing.manifest_path = module.manifest_path;
            }
        } else {
            settings.modules.push(module);
        }
    }
    Ok(())
}

fn load_theme_files(paths: &AppPaths, settings: &mut Settings) -> Result<(), ConfigError> {
    let directory = paths.config_dir.join("themes");
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(theme) = serde_json::from_str::<ThemeConfig>(&text) {
            settings
                .themes
                .entry(name.to_ascii_lowercase())
                .or_insert(theme);
        }
    }
    Ok(())
}

pub fn builtin_themes() -> &'static [&'static str] {
    &["monochrome", "ocean", "gruvbox", "nord", "dracula"]
}

pub fn theme_exists(settings: &Settings, name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    builtin_themes().contains(&name.as_str()) || settings.themes.contains_key(&name)
}

fn default_refresh_ms() -> u64 {
    100
}
fn default_theme() -> String {
    "monochrome".into()
}
fn default_image_mode() -> String {
    "auto".into()
}
fn default_player_command() -> String {
    "ffplay -nodisp -autoexit -loglevel quiet {file}".into()
}
fn default_skip_seconds() -> u64 {
    5
}
fn default_cover_size() -> u16 {
    16
}
fn default_quit() -> String {
    "q".into()
}
fn default_play() -> String {
    "space".into()
}
fn default_stop() -> String {
    "x".into()
}
fn default_search() -> String {
    "/".into()
}

fn default_theme_foreground() -> String {
    "#e6e6e6".into()
}
fn default_theme_background() -> String {
    "#101010".into()
}
fn default_theme_accent() -> String {
    "#5fd7ff".into()
}
fn default_theme_muted() -> String {
    "#808080".into()
}
fn default_theme_highlight() -> String {
    "#303030".into()
}
fn default_theme_playing() -> String {
    "#5fff87".into()
}
fn default_theme_border() -> String {
    "#606060".into()
}

#[allow(dead_code)]
fn _path_exists(path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    use super::{load, theme_exists, write_default, write_settings, Settings};
    use crate::paths::AppPaths;
    use std::fs;
    use tempfile::tempdir;

    fn paths(root: &std::path::Path) -> AppPaths {
        AppPaths {
            config_dir: root.join("config/groov"),
            settings_file: root.join("config/groov/settings.json"),
            data_dir: root.join("data/groov"),
            songs_dir: root.join("data/groov/songs"),
            cache_dir: root.join("cache/groov"),
        }
    }

    #[test]
    fn built_in_and_installed_themes_are_resolved() {
        let temp = tempdir().unwrap();
        let app_paths = paths(temp.path());
        fs::create_dir_all(&app_paths.config_dir).unwrap();
        write_default(&app_paths).unwrap();
        let mut settings = load(&app_paths).unwrap();
        settings.themes.insert("sunset".into(), Default::default());
        assert!(theme_exists(&settings, "GRUVBOX"));
        assert!(theme_exists(&settings, "sunset"));
        assert!(!theme_exists(&settings, "missing"));
    }

    #[test]
    fn module_manifest_is_discovered_with_working_directory() {
        let temp = tempdir().unwrap();
        let app_paths = paths(temp.path());
        fs::create_dir_all(app_paths.config_dir.join("modules/demo")).unwrap();
        fs::write(
            app_paths.config_dir.join("modules/demo/module.json"),
            r#"{"name":"demo","enabled":true,"command":"demo","commands":[{"name":"hello"}]}"#,
        )
        .unwrap();
        let settings = load(&app_paths).unwrap();
        assert_eq!(settings.modules.len(), 1);
        assert_eq!(settings.modules[0].name, "demo");
        assert!(settings.modules[0]
            .cwd
            .as_deref()
            .unwrap()
            .ends_with("modules/demo"));
        assert!(settings.modules[0]
            .manifest_path
            .as_ref()
            .unwrap()
            .ends_with("modules/demo/module.json"));
    }

    #[test]
    fn default_settings_are_serializable() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains("\"themes\""));
        assert!(json.contains("\"repeat\":false"));
        assert!(json.contains("\"follow_playback\":false"));
    }

    #[test]
    fn reference_module_manifest_has_extension_contract() {
        let module: super::ModuleConfig =
            serde_json::from_str(include_str!("../examples/modules/hello/module.json")).unwrap();
        assert_eq!(module.name, "hello");
        assert_eq!(module.version, "1.0.0");
        assert!(!module.enabled);
        assert_eq!(module.commands[0].name, "hello");
        assert_eq!(module.keys.get("H").map(String::as_str), Some("hello"));
    }

    #[test]
    fn rpc_reference_manifest_enables_jsonl_views() {
        let module: super::ModuleConfig = serde_json::from_str(include_str!(
            "../examples/modules/rpc_dashboard/module.json"
        ))
        .unwrap();
        assert!(module.rpc);
        assert!(module.session);
        assert_eq!(module.commands[0].name, "dashboard");
        assert_eq!(module.keys.get("D").map(String::as_str), Some("dashboard"));
    }

    #[test]
    fn older_settings_load_with_new_defaults() {
        let temp = tempdir().unwrap();
        let app_paths = paths(temp.path());
        fs::create_dir_all(&app_paths.config_dir).unwrap();
        fs::write(
            &app_paths.settings_file,
            r#"{"refresh_ms":100,"theme":"monochrome","player_command":"true"}"#,
        )
        .unwrap();
        let settings = load(&app_paths).unwrap();
        assert!(!settings.repeat);
        assert!(!settings.follow_playback);
        assert!(settings.modules.is_empty());
        assert_eq!(settings.cover_size, 16);
    }

    #[test]
    fn unknown_settings_keys_survive_a_round_trip() {
        let temp = tempdir().unwrap();
        let app_paths = paths(temp.path());
        fs::create_dir_all(&app_paths.config_dir).unwrap();
        fs::write(
            &app_paths.settings_file,
            r#"{"future_feature":{"enabled":true}}"#,
        )
        .unwrap();
        let settings = load(&app_paths).unwrap();
        write_settings(&app_paths, &settings).unwrap();
        let text = fs::read_to_string(app_paths.settings_file).unwrap();
        assert!(text.contains("future_feature"));
    }
}
