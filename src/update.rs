use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const API_URL: &str = "https://api.github.com/repos/bluetoothpaste1/groov/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Auto,
    Checkout,
    Release,
}

#[derive(Debug, Clone, Copy)]
struct Options {
    check: bool,
    source: Source,
    build: bool,
}

pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let options = parse_options(&args)?;
    let checkout = find_checkout();
    match (options.source, checkout) {
        (Source::Checkout, Some(root)) | (Source::Auto, Some(root)) => {
            update_checkout(&root, options)
        }
        (Source::Checkout, None) => Err(
            "cannot find a groov git checkout; run update inside the repository or use --source release"
                .into(),
        ),
        (Source::Release, _) | (Source::Auto, None) => update_release(options),
    }
}

fn print_help() {
    println!("Usage: groov update [--check] [--source auto|checkout|release] [--no-build]");
    println!("  auto      update a checkout when one is detected, otherwise GitHub release");
    println!("  checkout  fast-forward the current git checkout and build it");
    println!("  release   download and atomically install the latest GitHub release");
    println!("  --check   inspect available updates without changing files");
    println!("  --no-build only update checkout source; skip cargo build");
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        check: false,
        source: Source::Auto,
        build: true,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => options.check = true,
            "--no-build" => options.build = false,
            "--source" | "--from" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("update --source requires auto, checkout, or release")?;
                options.source = match value.to_ascii_lowercase().as_str() {
                    "auto" => Source::Auto,
                    "checkout" | "git" => Source::Checkout,
                    "release" | "binary" => Source::Release,
                    _ => return Err("update --source must be auto, checkout, or release".into()),
                };
            }
            other => return Err(format!("unknown update option '{other}'")),
        }
        index += 1;
    }
    Ok(options)
}

fn find_checkout() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(current) = env::current_dir() {
        candidates.push(current);
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.to_path_buf());
        }
    }
    for candidate in candidates {
        for root in candidate.ancestors() {
            if root.join("Cargo.toml").is_file()
                && root.join(".git").exists()
                && fs::read_to_string(root.join("Cargo.toml"))
                    .map(|text| text.contains("name = \"groov\""))
                    .unwrap_or(false)
            {
                return Some(root.to_path_buf());
            }
        }
    }
    None
}

fn update_checkout(root: &Path, options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let dirty = git(root, &["status", "--porcelain"])?;
    if !dirty.trim().is_empty() {
        return Err(format!(
            "checkout has local changes; commit or stash them before groov update:\n{}",
            dirty.trim_end()
        )
        .into());
    }
    let branch = git(root, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("checkout is detached; switch to a branch before groov update".into());
    }
    git(root, &["fetch", "--tags", "--prune"])?;
    let upstream = git(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| format!("origin/{branch}"));
    let before = git(root, &["rev-parse", "HEAD"])?;
    let remote = git(root, &["rev-parse", upstream.trim()])?;
    if before.trim() == remote.trim() {
        println!("groov checkout is already up to date ({})", before.trim());
        if options.build && !options.check {
            build_checkout(root)?;
        }
        return Ok(());
    }
    let count = git(
        root,
        &["rev-list", "--count", &format!("HEAD..{}", upstream.trim())],
    )
    .map(|value| value.lines().next().unwrap_or("unknown").trim().to_string())
    .unwrap_or_else(|_| "unknown".into());
    if options.check {
        println!(
            "groov checkout has {count} available commit(s) from {}",
            upstream.trim()
        );
        return Ok(());
    }
    git(root, &["merge", "--ff-only", upstream.trim()])?;
    let after = git(root, &["rev-parse", "HEAD"])?;
    println!("updated checkout {} -> {}", before.trim(), after.trim());
    if options.build {
        build_checkout(root)?;
    }
    println!("user settings and library remain in their XDG directories");
    Ok(())
}

fn build_checkout(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let executable = env::current_exe().ok();
    let debug_binary = executable
        .as_ref()
        .is_some_and(|path| path.starts_with(root.join("target").join("debug")));
    let mut args = vec!["build", "--locked"];
    if !debug_binary {
        args.push("--release");
    }
    run_command(root, "cargo", &args)?;
    println!(
        "built {} binary",
        if debug_binary { "debug" } else { "release" }
    );
    Ok(())
}

fn update_release(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(target_os = "windows") {
        return Err("release self-update is currently supported on macOS and Linux; update the checkout on Windows".into());
    }
    let release = latest_release()?;
    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("GitHub release response has no tag_name")?;
    let version = tag.trim_start_matches('v');
    if version == env!("CARGO_PKG_VERSION") {
        println!("groov {} is already the latest release", version);
        return Ok(());
    }
    let asset = select_asset(&release, tag).ok_or_else(|| {
        format!(
            "no release archive matches this platform (os={}, arch={})",
            env::consts::OS,
            platform_arch()
        )
    })?;
    let name = asset
        .get("name")
        .and_then(Value::as_str)
        .ok_or("release asset has no name")?;
    if Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name) {
        return Err("release asset has an unsafe filename".into());
    }
    let url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .ok_or("release asset has no download URL")?;
    if options.check {
        println!("groov update available: {} ({})", tag, name);
        return Ok(());
    }
    let temp = unique_temp_dir("groov-update")?;
    let archive = temp.join(name);
    download(url, &archive)?;
    if let Some(checksum) = checksum_asset(&release, name) {
        let checksum_path = temp.join(format!("{name}.sha256"));
        download(checksum, &checksum_path)?;
        verify_checksum(&archive, &checksum_path)?;
    }
    run_command(&temp, "tar", &["-xzf", name])?;
    let binary = find_binary(&temp)?;
    let destination = fs::canonicalize(env::current_exe()?)?;
    let temporary =
        destination.with_file_name(format!(".groov-update-{}-{}", std::process::id(), version));
    fs::copy(binary, &temporary)?;
    set_executable(&temporary)?;
    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot replace {}: {error}", destination.display()).into());
    }
    let _ = fs::remove_dir_all(&temp);
    println!("updated groov to {tag} at {}", destination.display());
    println!("settings, modules, themes, cache, and songs were preserved");
    Ok(())
}

fn latest_release() -> Result<Value, Box<dyn std::error::Error>> {
    let body = curl(API_URL)?;
    Ok(serde_json::from_str(&body)?)
}

fn select_asset<'a>(release: &'a Value, tag: &str) -> Option<&'a Value> {
    let os = match env::consts::OS {
        "macos" => "macos",
        "linux" => "linux-x86_64",
        other => other,
    };
    let arch = platform_arch();
    release
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets.iter().find(|asset| {
                let Some(name) = asset.get("name").and_then(Value::as_str) else {
                    return false;
                };
                name.ends_with(".tar.gz")
                    && name.contains(&format!("groov-{tag}-"))
                    && name.contains(os)
                    && name.contains(arch)
            })
        })
}

fn platform_arch() -> &'static str {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "arm64",
        (_, arch) => arch,
    }
}

fn checksum_asset<'a>(release: &'a Value, archive: &str) -> Option<&'a str> {
    let assets = release.get("assets").and_then(Value::as_array)?;
    assets
        .iter()
        .find_map(|asset| {
            (asset.get("name").and_then(Value::as_str) == Some(&format!("{archive}.sha256")))
                .then(|| asset.get("browser_download_url").and_then(Value::as_str))
                .flatten()
        })
        .or_else(|| {
            assets.iter().find_map(|asset| {
                (asset.get("name").and_then(Value::as_str) == Some("SHA256SUMS"))
                    .then(|| asset.get("browser_download_url").and_then(Value::as_str))
                    .flatten()
            })
        })
}

fn find_binary(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("groov") {
                return Ok(path);
            }
        }
    }
    Err("release archive does not contain a groov executable".into())
}

fn unique_temp_dir(prefix: &str) -> io::Result<PathBuf> {
    let path = env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }
    fs::create_dir(&path)?;
    Ok(path)
}

fn download(url: &str, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let destination = destination.to_string_lossy().into_owned();
    run_command(
        Path::new("."),
        "curl",
        &[
            "-fL",
            "--retry",
            "2",
            "-A",
            "groov-update",
            "-o",
            &destination,
            url,
        ],
    )
}

fn curl(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-A", "groov-update", url])
        .output()
        .map_err(|error| format!("cannot run curl: {error}"))?;
    if !output.status.success() {
        return Err(format!("curl failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn verify_checksum(archive: &Path, checksum: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let archive_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("archive has no valid filename")?;
    let checksum_text = fs::read_to_string(checksum)?;
    let expected = expected_checksum(&checksum_text, archive_name)
        .ok_or("checksum file has no entry for the downloaded archive")?;
    let archive = archive.to_string_lossy();
    let output = if Command::new("shasum").arg("--version").output().is_ok() {
        Command::new("shasum")
            .args(["-a", "256", &archive])
            .output()?
    } else {
        Command::new("sha256sum").arg(&*archive).output()?
    };
    if !output.status.success() {
        return Err("cannot calculate release checksum (install shasum or sha256sum)".into());
    }
    let actual = String::from_utf8(output.stdout)?
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if actual != expected {
        return Err(format!("release checksum mismatch: expected {expected}, got {actual}").into());
    }
    Ok(())
}

fn expected_checksum(contents: &str, archive_name: &str) -> Option<String> {
    contents
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.as_slice() {
                [hash] if !hash.is_empty() => Some(*hash),
                [hash, file]
                    if Path::new(file.trim_start_matches('*'))
                        .file_name()
                        .and_then(|name| name.to_str())
                        == Some(archive_name) =>
                {
                    Some(*hash)
                }
                _ => None,
            }
        })
        .next()
        .map(str::to_ascii_lowercase)
}

fn set_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    run_command_output(root, "git", args)
}

fn run_command(
    root: &Path,
    command: &str,
    args: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(command)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {command}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}

fn run_command_output(
    root: &Path,
    command: &str,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new(command)
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {command}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::{expected_checksum, parse_options, platform_arch, select_asset, Source};
    use serde_json::json;

    #[test]
    fn update_options_support_check_and_sources() {
        let options =
            parse_options(&["--check".into(), "--source".into(), "release".into()]).unwrap();
        assert!(options.check);
        assert_eq!(options.source, Source::Release);
        assert!(options.build);
    }

    #[test]
    fn release_asset_selection_matches_platform_archive() {
        let tag = "v1.5.0";
        let release = json!({
            "assets": [
                {"name":"groov-v1.5.0-other-x86_64.tar.gz"},
                {"name": format!("groov-{tag}-{}-{}.tar.gz", if cfg!(target_os = "macos") { "macos" } else { "linux-x86_64" }, platform_arch())}
            ]
        });
        assert!(select_asset(&release, tag).is_some());
    }

    #[test]
    fn checksum_manifest_selects_the_downloaded_archive() {
        let contents = "111111  other.tar.gz\nABCDEF  release/groov-v1.5.0-linux.tar.gz\n";
        assert_eq!(
            expected_checksum(contents, "groov-v1.5.0-linux.tar.gz").as_deref(),
            Some("abcdef")
        );
    }
}
