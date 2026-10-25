use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::tempdir;

fn run_cli(root: &Path, args: &[&str]) -> std::process::Output {
    let config = root.join("config");
    let data = root.join("data");
    let cache = root.join("cache");
    Command::new(env!("CARGO_BIN_EXE_groov"))
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_DATA_HOME", data)
        .env("XDG_CACHE_HOME", cache)
        .args(args)
        .output()
        .expect("groov command should start")
}

fn run_cli_with_input(root: &Path, args: &[&str], input: &str) -> std::process::Output {
    let config = root.join("config");
    let data = root.join("data");
    let cache = root.join("cache");
    let mut child = Command::new(env!("CARGO_BIN_EXE_groov"))
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_DATA_HOME", data)
        .env("XDG_CACHE_HOME", cache)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("groov command should start");
    child
        .stdin
        .take()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("confirmation should be written");
    child.wait_with_output().expect("groov command should exit")
}

#[test]
fn cli_initializes_and_lists_an_empty_library() {
    let temp = tempdir().unwrap();
    let init = run_cli(temp.path(), &["init"]);
    assert!(init.status.success(), "init failed: {:?}", init);

    let list = run_cli(temp.path(), &["list", "--json"]);
    assert!(list.status.success(), "list failed: {:?}", list);
    assert_eq!(String::from_utf8_lossy(&list.stdout).trim(), "[]");
}

#[test]
fn cli_manages_a_discovered_module() {
    let temp = tempdir().unwrap();
    let module_dir = temp.path().join("config/groov/modules/demo");
    fs::create_dir_all(&module_dir).unwrap();
    fs::write(
        module_dir.join("module.json"),
        r#"{
            "name":"demo",
            "version":"1.0.0",
            "description":"CLI test module",
            "enabled":false,
            "command":"demo"
        }"#,
    )
    .unwrap();

    let info = run_cli(temp.path(), &["modules", "info", "demo"]);
    assert!(info.status.success(), "info failed: {:?}", info);
    let info_text = String::from_utf8_lossy(&info.stdout);
    assert!(info_text.contains("module.json"));
    assert!(info_text.contains("CLI test module"));

    let on = run_cli(temp.path(), &["modules", "on", "demo"]);
    assert!(on.status.success(), "on failed: {:?}", on);
    let settings = fs::read_to_string(temp.path().join("config/groov/settings.json")).unwrap();
    assert!(settings.contains("\"enabled\": true"));

    let off = run_cli(temp.path(), &["modules", "off", "demo"]);
    assert!(off.status.success(), "off failed: {:?}", off);
}

#[test]
fn cli_can_select_a_builtin_theme() {
    let temp = tempdir().unwrap();
    let init = run_cli(temp.path(), &["init"]);
    assert!(init.status.success());
    let use_theme = run_cli(temp.path(), &["theme", "use", "ocean"]);
    assert!(use_theme.status.success(), "theme failed: {:?}", use_theme);
    let settings = fs::read_to_string(temp.path().join("config/groov/settings.json")).unwrap();
    assert!(settings.contains("\"theme\": \"ocean\""));
}

#[test]
fn cli_loads_and_selects_a_custom_theme_file() {
    let temp = tempdir().unwrap();
    let themes = temp.path().join("config/groov/themes");
    fs::create_dir_all(&themes).unwrap();
    fs::write(
        themes.join("sunset.json"),
        r##"{
            "name": "sunset",
            "colors": {
                "foreground": "#fff4e6",
                "background": "reset",
                "accent": "#ff9f43",
                "muted": "#b8a99a",
                "highlight": "reset",
                "playing": "#ffd166",
                "border": "#d6c2b5"
            }
        }"##,
    )
    .unwrap();

    let list = run_cli(temp.path(), &["theme", "list"]);
    assert!(list.status.success(), "theme list failed: {:?}", list);
    assert!(String::from_utf8_lossy(&list.stdout).contains("sunset"));

    let use_theme = run_cli(temp.path(), &["theme", "use", "sunset"]);
    assert!(
        use_theme.status.success(),
        "theme use failed: {:?}",
        use_theme
    );
    let settings = fs::read_to_string(temp.path().join("config/groov/settings.json")).unwrap();
    assert!(settings.contains("\"theme\": \"sunset\""));
}

#[test]
fn cli_installs_enables_disables_and_removes_module() {
    let temp = tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("basic_modules/flame");
    let source = source.to_string_lossy().into_owned();

    let install = run_cli(temp.path(), &["module", "add", &source]);
    assert!(install.status.success(), "install failed: {:?}", install);
    let installed = temp.path().join("config/groov/modules/flame/module.json");
    assert!(installed.is_file());

    let enable = run_cli(temp.path(), &["modules", "enable", "flame"]);
    assert!(enable.status.success(), "enable failed: {:?}", enable);
    let settings = fs::read_to_string(temp.path().join("config/groov/settings.json")).unwrap();
    assert!(settings.contains("\"enabled\": true"));

    let disable = run_cli(temp.path(), &["modules", "disable", "flame"]);
    assert!(disable.status.success(), "disable failed: {:?}", disable);
    let settings = fs::read_to_string(temp.path().join("config/groov/settings.json")).unwrap();
    assert!(settings.contains("\"enabled\": false"));

    let remove = run_cli(temp.path(), &["modules", "remove", "flame"]);
    assert!(remove.status.success(), "remove failed: {:?}", remove);
    assert!(!installed.exists());
}

#[test]
fn cli_reports_the_release_version() {
    let temp = tempdir().unwrap();
    let version = run_cli(temp.path(), &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "groov 1.5.0"
    );
}

#[test]
fn cli_update_help_describes_safe_sources() {
    let temp = tempdir().unwrap();
    let update = run_cli(temp.path(), &["update", "--help"]);
    assert!(update.status.success(), "update help failed: {:?}", update);
    let text = String::from_utf8_lossy(&update.stdout);
    assert!(text.contains("--source auto|checkout|release"));
    assert!(text.contains("--check"));
}

#[test]
fn cli_uninstall_requires_confirmation_and_removes_all_user_data() {
    let temp = tempdir().unwrap();
    let init = run_cli(temp.path(), &["init"]);
    assert!(init.status.success(), "init failed: {:?}", init);

    let config_dir = temp.path().join("config/groov");
    let data_dir = temp.path().join("data/groov");
    let cache_dir = temp.path().join("cache/groov");
    fs::create_dir_all(data_dir.join("songs/1")).unwrap();
    fs::write(config_dir.join("settings.json"), b"settings").unwrap();
    fs::write(data_dir.join("songs/1/song.mp3"), b"audio").unwrap();
    fs::write(data_dir.join("songs/1/image.jpg"), b"cover").unwrap();
    fs::write(cache_dir.join("art.cache"), b"cache").unwrap();

    let cancelled = run_cli_with_input(temp.path(), &["uninstall"], "n\n");
    assert!(cancelled.status.success(), "cancel failed: {:?}", cancelled);
    let cancelled_text = String::from_utf8_lossy(&cancelled.stdout);
    assert!(cancelled_text.contains("WARNING"));
    assert!(cancelled_text.contains("cancelled"));
    assert!(config_dir.exists());
    assert!(data_dir.join("songs/1/image.jpg").exists());
    assert!(cache_dir.exists());

    let removed = run_cli_with_input(temp.path(), &["uninstall"], "yes\n");
    assert!(removed.status.success(), "uninstall failed: {:?}", removed);
    let removed_text = String::from_utf8_lossy(&removed.stdout);
    assert!(removed_text.contains("user data removed"));
    assert!(!config_dir.exists());
    assert!(!data_dir.exists());
    assert!(!cache_dir.exists());
}
