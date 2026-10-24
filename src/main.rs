mod config;
mod library;
mod modules;
mod paths;
mod player;
mod tui;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use config::Settings;
use library::{load_song, next_id, scan, write_info_atomic, SongInfo};
use paths::AppPaths;

fn main() {
    if let Err(error) = run() {
        eprintln!("groov: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "tui".into());
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }
    if matches!(command.as_str(), "version" | "--version" | "-V") {
        println!("groov {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let paths = AppPaths::discover()?;
    match command.as_str() {
        "init" => init(&paths)?,
        "uninstall" => uninstall(&paths, args.collect())?,
        "paths" => println!("{}", paths.describe()),
        "doctor" => doctor(&paths)?,
        "theme" | "themes" => theme_command(&paths, args.collect())?,
        "modules" => modules_command(&paths, args.collect())?,
        "list" => list(&paths, args.collect())?,
        "scan" => scan_command(&paths)?,
        "add" | "import" => add(&paths, args.collect())?,
        "edit" => edit(&paths, args.collect())?,
        "tui" => tui_command(&paths)?,
        "tracks" => {
            eprintln!("groov: VK support is preserved in the legacy C target; local library commands are now the default");
            print_help();
        }
        other => return Err(format!("unknown command '{other}' (try groov help)").into()),
    }
    Ok(())
}

fn print_help() {
    println!("groov — minimal Vim-like terminal music player");
    println!();
    println!("Usage: groov [command]");
    println!("       groov --version");
    println!();
    println!("Commands:");
    println!("  tui                         open the player (default)");
    println!("  init                        create config and data directories");
    println!("  uninstall                   permanently remove groov data (asks for confirmation)");
    println!("  add FILE [options]          move audio into a new song directory (--copy/--dry-run available)");
    println!("  edit ID [options]           update title, artist, album, description, tags, cover, lyrics");
    println!("  list [--tag TAG]...         list songs, optionally filtered by all tags");
    println!("  scan                        validate and count the local library");
    println!("  paths                       print resolved XDG paths");
    println!("  doctor                      show setup and player diagnostics");
    println!("  version                     print the groov version");
    println!("  theme list|install|use      list, install, or activate a color theme");
    println!("  modules [on|off|info] NAME  list or manage installed modules");
    println!();
    println!(
        "TUI: j/k move, Enter plays, n/] next, [ prev, a enqueue, / search, : commands, q quits"
    );
}

fn theme_command(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    let mut settings = config::ensure(paths)?;
    match args.first().map(String::as_str).unwrap_or("list") {
        "list" => {
            for name in config::builtin_themes() {
                println!(
                    "{name}{}",
                    if settings.theme.eq_ignore_ascii_case(name) {
                        " *"
                    } else {
                        ""
                    }
                );
            }
            for name in settings.themes.keys() {
                println!(
                    "{name}{}",
                    if settings.theme.eq_ignore_ascii_case(name) {
                        " *"
                    } else {
                        ""
                    }
                );
            }
        }
        "install" => {
            let file = args.get(1).ok_or("theme install requires a JSON file")?;
            let text = fs::read_to_string(file)?;
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_ascii_lowercase)
                .or_else(|| {
                    Path::new(file)
                        .file_stem()
                        .and_then(|v| v.to_str())
                        .map(str::to_ascii_lowercase)
                })
                .ok_or("theme file needs a name or filename stem")?;
            if name.is_empty()
                || !name.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
            {
                return Err("theme name must contain only letters, numbers, '-' or '_'".into());
            }
            let theme: config::ThemeConfig =
                serde_json::from_value(value.get("colors").cloned().unwrap_or(value))?;
            settings.themes.insert(name.clone(), theme);
            config::write_settings(paths, &settings)?;
            println!("installed theme {name}");
        }
        "use" | "set" => {
            let name = args
                .get(1)
                .ok_or("theme use requires a theme name")?
                .to_ascii_lowercase();
            if !config::theme_exists(&settings, &name) {
                return Err(format!("unknown theme '{name}' (use groov theme list)").into());
            }
            settings.theme = name.clone();
            config::write_settings(paths, &settings)?;
            println!("active theme {name}");
        }
        other => {
            return Err(
                format!("unknown theme command '{other}' (use list, install, or use)").into(),
            )
        }
    }
    Ok(())
}

fn modules_command(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    let mut settings = config::ensure(paths)?;
    let action = args.first().map(|value| value.to_ascii_lowercase());
    match action.as_deref() {
        None | Some("list") => {
            if settings.modules.is_empty() {
                println!("no modules installed");
            } else {
                for module in settings.modules {
                    println!(
                        "{}  {}{}",
                        module.name,
                        module.command.as_deref().unwrap_or("manifest only"),
                        if module.enabled {
                            " (enabled)"
                        } else {
                            " (disabled)"
                        }
                    );
                }
            }
        }
        Some(command) if command == "on" || command == "off" => {
            let name = args.get(1).ok_or("modules on/off requires a module name")?;
            let enabled = command == "on";
            let Some(module) = settings
                .modules
                .iter_mut()
                .find(|module| module.name.eq_ignore_ascii_case(name))
            else {
                return Err(format!("module not found: {name}").into());
            };
            module.enabled = enabled;
            let module_name = module.name.clone();
            config::write_settings(paths, &settings)?;
            println!(
                "module {module_name}: {}",
                if enabled { "on" } else { "off" }
            );
        }
        Some("info") => {
            let name = args.get(1).ok_or("modules info requires a module name")?;
            let Some(module) = settings
                .modules
                .iter()
                .find(|module| module.name.eq_ignore_ascii_case(name))
            else {
                return Err(format!("module not found: {name}").into());
            };
            let source = module
                .manifest_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| paths.settings_file.display().to_string());
            println!("source: {source}");
            println!(
                "working directory: {}",
                module.cwd.as_deref().unwrap_or("(inherited)")
            );
            println!("{}", serde_json::to_string_pretty(module)?);
        }
        Some(command) => {
            return Err(
                format!("unknown modules command '{command}' (use list, on, off, or info)").into(),
            )
        }
    }
    Ok(())
}

fn init(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    if !paths.settings_file.exists() {
        config::write_default(paths)?;
        println!("created {}", paths.settings_file.display());
    } else {
        println!("kept {}", paths.settings_file.display());
    }
    println!("data directory ready at {}", paths.data_dir.display());
    Ok(())
}

fn uninstall(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if !args.is_empty() {
        return Err("uninstall takes no arguments".into());
    }
    println!("WARNING: this permanently deletes all groov user data.");
    println!("It removes the library (audio, covers, metadata and lyrics), configuration, modules, themes, and cache.");
    println!();
    println!("The following directories will be removed:");
    println!("{}", paths.describe());
    print!("Continue with uninstall? [y/N] ");
    io::stdout().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("uninstall cancelled; no files were changed");
        return Ok(());
    }

    for (label, path) in [
        ("configuration", paths.config_dir.as_path()),
        ("data", paths.data_dir.as_path()),
        ("cache", paths.cache_dir.as_path()),
    ] {
        if remove_path(path)? {
            println!("removed {label}: {}", path.display());
        } else {
            println!("not found {label}: {}", path.display());
        }
    }
    println!("groov user data removed");
    Ok(())
}

fn remove_path(path: &Path) -> Result<bool, io::Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(true)
}

fn doctor(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    let settings = config::load(paths)?;
    println!("{}", paths.describe());
    println!(
        "settings {}",
        if paths.settings_file.is_file() {
            "ok"
        } else {
            "missing (run groov init)"
        }
    );
    println!("player  {}", settings.player_command);
    println!("repeat  {}", if settings.repeat { "on" } else { "off" });
    println!("library {}", scan(paths)?.len());
    println!("theme   {}", settings.theme);
    println!(
        "modules {}",
        settings
            .modules
            .iter()
            .filter(|module| module.enabled)
            .count()
    );
    println!(
        "images  {}",
        match settings.image_mode.to_ascii_lowercase().as_str() {
            "auto" | "ansi" | "ascii" => "ANSI halfblocks (portable)",
            "sixel" | "kitty" | "iterm" | "iterm2" => "explicit graphics protocol",
            "none" => "disabled",
            _ => "ANSI halfblocks (fallback)",
        }
    );
    Ok(())
}

fn list(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    let mut tags = Vec::new();
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--tag" => {
                index += 1;
                let value = args.get(index).ok_or("--tag requires a value")?;
                tags.push(value.to_ascii_lowercase());
            }
            "--json" => json = true,
            other => return Err(format!("unknown list option '{other}'").into()),
        }
        index += 1;
    }
    let songs = scan(paths)?;
    let songs: Vec<_> = songs
        .into_iter()
        .filter(|song| song.info.matches_tags(&tags))
        .collect();
    if json {
        let values: Vec<_> = songs.iter().map(|song| &song.info).collect();
        println!("{}", serde_json::to_string_pretty(&values)?);
    } else {
        for song in songs {
            println!(
                "{:>4}  {:<24}  {}  [{}]",
                song.info.id,
                song.info.artist,
                song.info.title,
                song.info.tags.join(",")
            );
        }
    }
    Ok(())
}

fn scan_command(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    let songs = scan(paths)?;
    println!(
        "{} valid song(s) in {}",
        songs.len(),
        paths.songs_dir.display()
    );
    Ok(())
}

fn edit(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let id = args.first().ok_or("edit requires a song id")?.clone();
    paths.ensure()?;
    let song = load_song(paths, &id)?;
    let mut info = song.info;
    let mut changed = false;
    let mut index = 1;
    while index < args.len() {
        let option = args[index].as_str();
        match option {
            "--clear-tags" => {
                info.tags.clear();
                changed = true;
                index += 1;
            }
            "--title" | "--artist" | "--album" | "--desc" | "--description" | "--tag"
            | "--untag" | "--image" | "--lyrics" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{option} requires a value"))?;
                match option {
                    "--title" => info.title = value.clone(),
                    "--artist" => info.artist = value.clone(),
                    "--album" => info.album = value.clone(),
                    "--desc" | "--description" => info.description = value.clone(),
                    "--tag" => {
                        for tag in value.split(',') {
                            info.add_tag(tag);
                        }
                    }
                    "--untag" => {
                        for tag in value.split(',') {
                            info.remove_tag(tag);
                        }
                    }
                    "--image" => {
                        let image_name = copy_cover(&song.directory, value)?;
                        info.image = image_name;
                    }
                    "--lyrics" => {
                        let lyrics_path = Path::new(value);
                        if !lyrics_path.is_file() {
                            return Err(format!("lyrics not found: {value}").into());
                        }
                        fs::copy(lyrics_path, song.directory.join("lyrics.txt"))?;
                        info.lyrics = "lyrics.txt".into();
                    }
                    _ => unreachable!(),
                }
                changed = true;
                index += 1;
            }
            other => return Err(format!("unknown edit option '{other}'").into()),
        }
    }
    if !changed {
        return Err("edit needs at least one metadata option".into());
    }
    info = info.normalized(&id);
    write_info_atomic(&song.directory.join("info.json"), &info)?;
    println!("updated song {id}");
    Ok(())
}

fn add(paths: &AppPaths, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let source = args.first().ok_or("add requires an audio file")?;
    let source_path = Path::new(source);
    if !source_path.is_file() {
        return Err(format!("audio file not found: {source}").into());
    }
    paths.ensure()?;
    let mut id = None;
    let mut title = String::new();
    let mut artist = String::new();
    let mut album = String::new();
    let mut description = String::new();
    let mut tags = Vec::new();
    let mut image = None;
    let mut lyrics = None;
    let mut move_source = true;
    let mut dry_run = false;
    let mut index = 1;
    while index < args.len() {
        let option = args[index].as_str();
        index += 1;
        match option {
            "--move" => {
                move_source = true;
                continue;
            }
            "--copy" => {
                move_source = false;
                continue;
            }
            "--dry-run" => {
                dry_run = true;
                continue;
            }
            _ => {}
        }
        let value = args
            .get(index)
            .ok_or_else(|| format!("{option} requires a value"))?;
        match option {
            "--id" => id = Some(value.clone()),
            "--title" => title = value.clone(),
            "--artist" => artist = value.clone(),
            "--album" => album = value.clone(),
            "--desc" | "--description" => description = value.clone(),
            "--tag" => tags.push(value.to_ascii_lowercase()),
            "--image" => image = Some(value.clone()),
            "--lyrics" => lyrics = Some(value.clone()),
            other => return Err(format!("unknown add option '{other}'").into()),
        }
        index += 1;
    }
    let id = id.unwrap_or(next_id(paths)?);
    if id.is_empty()
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err("id must contain only letters, numbers, '-' or '_'".into());
    }
    let directory = paths.song_dir(&id);
    if directory.exists() {
        return Err(format!("song id already exists: {id}").into());
    }
    if let Some(image_source) = image.as_deref() {
        if !Path::new(image_source).is_file() {
            return Err(format!("cover not found: {image_source}").into());
        }
        validate_cover_extension(image_source)?;
    }
    if let Some(lyrics_source) = lyrics.as_deref() {
        if !Path::new(lyrics_source).is_file() {
            return Err(format!("lyrics not found: {lyrics_source}").into());
        }
    }
    let metadata = probe_audio(source_path);
    let title = if title.is_empty() {
        if metadata.title.is_empty() {
            source_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Untitled")
                .to_string()
        } else {
            metadata.title
        }
    } else {
        title
    };
    let artist = if artist.is_empty() {
        metadata.artist
    } else {
        artist
    };
    let album = if album.is_empty() {
        metadata.album
    } else {
        album
    };
    if dry_run {
        println!("source      {}", source_path.display());
        println!("destination {}", directory.join("song.mp3").display());
        println!("mode        {}", if move_source { "move" } else { "copy" });
        println!(
            "title       {}",
            if title.is_empty() { "Untitled" } else { &title }
        );
        println!(
            "artist      {}",
            if artist.is_empty() {
                "Unknown artist"
            } else {
                &artist
            }
        );
        println!(
            "album       {}",
            if album.is_empty() { "-" } else { &album }
        );
        println!("duration    {}s", metadata.duration_seconds);
        return Ok(());
    }
    let staging = paths
        .songs_dir
        .join(format!(".import-{id}-{}", std::process::id()));
    if staging.exists() {
        return Err(format!(
            "temporary import directory already exists: {}",
            staging.display()
        )
        .into());
    }
    fs::create_dir(&staging)?;
    let import_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::copy(source_path, staging.join("song.mp3"))?;
        let image_name = if let Some(image_source) = image.as_deref() {
            Some(copy_cover(&staging, image_source)?)
        } else {
            None
        };
        if let Some(path) = lyrics {
            fs::copy(path, staging.join("lyrics.txt"))?;
        }
        let info = SongInfo {
            id: id.clone(),
            artist,
            lyrics: if staging.join("lyrics.txt").is_file() {
                "lyrics.txt".into()
            } else {
                String::new()
            },
            title,
            album,
            description,
            image: image_name.unwrap_or_default(),
            tags,
            duration_seconds: metadata.duration_seconds,
            added_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_secs()
                .to_string(),
        }
        .normalized(&id);
        write_info_atomic(&staging.join("info.json"), &info)?;
        fs::rename(&staging, &directory)?;
        Ok(())
    })();
    if let Err(error) = import_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if move_source {
        if let Err(error) = fs::remove_file(source_path) {
            eprintln!("groov: imported the song but could not remove the source: {error}");
        }
    }
    println!(
        "{} {} as {}",
        if move_source { "moved" } else { "copied" },
        source_path.display(),
        directory.display()
    );
    Ok(())
}

#[derive(Default)]
struct ProbedAudio {
    title: String,
    artist: String,
    album: String,
    duration_seconds: u64,
}

fn probe_audio(path: &Path) -> ProbedAudio {
    let output = match Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:format_tags=title,artist,album",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return ProbedAudio::default(),
    };
    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return ProbedAudio::default(),
    };
    let format = json.get("format").unwrap_or(&serde_json::Value::Null);
    let tags = format.get("tags").unwrap_or(&serde_json::Value::Null);
    let text = |key: &str| {
        tags.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let duration_seconds = format
        .get("duration")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.round() as u64)
        .unwrap_or(0);
    ProbedAudio {
        title: text("title"),
        artist: text("artist"),
        album: text("album"),
        duration_seconds,
    }
}

fn copy_cover(directory: &Path, source: &str) -> Result<String, Box<dyn std::error::Error>> {
    let source_path = Path::new(source);
    if !source_path.is_file() {
        return Err(format!("cover not found: {source}").into());
    }
    let extension = validate_cover_extension(source)?;
    let name = format!("image.{extension}");
    let destination = directory.join(&name);
    if source_path != destination {
        fs::copy(source_path, destination)?;
    }
    Ok(name)
}

fn validate_cover_extension(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    let source_path = Path::new(source);
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("jpg")
        .to_ascii_lowercase();
    if !["jpg", "jpeg", "png", "webp"].contains(&extension.as_str()) {
        return Err("cover must be jpg, jpeg, png, or webp".into());
    }
    Ok(extension)
}

fn tui_command(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure()?;
    let settings: Settings = config::ensure(paths)?;
    let songs = scan(paths)?;
    tui::run(paths.clone(), settings, songs)
}
