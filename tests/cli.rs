use std::fs;
use std::path::Path;
use std::process::Command;

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
fn cli_reports_the_release_version() {
    let temp = tempdir().unwrap();
    let version = run_cli(temp.path(), &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "groov 1.0.0"
    );
}
