use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::paths::AppPaths;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0}")]
    Message(String),
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("cannot write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SongInfo {
    #[serde(default, rename = "Id")]
    pub id: String,
    #[serde(default, rename = "Artist")]
    pub artist: String,
    #[serde(default, rename = "Lyrics")]
    pub lyrics: String,
    #[serde(default, rename = "Title")]
    pub title: String,
    #[serde(default, rename = "Album")]
    pub album: String,
    #[serde(default, rename = "Description", alias = "Desc")]
    pub description: String,
    #[serde(default, rename = "Image")]
    pub image: String,
    #[serde(default, rename = "Tags")]
    pub tags: Vec<String>,
    #[serde(default, rename = "Duration")]
    pub duration_seconds: u64,
    #[serde(default, rename = "AddedAt")]
    pub added_at: String,
}

impl SongInfo {
    pub fn normalized(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self.tags = self
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        self.tags.sort();
        self.tags.dedup();
        if self.title.is_empty() {
            self.title = "Untitled".to_string();
        }
        if self.artist.is_empty() {
            self.artist = "Unknown artist".to_string();
        }
        self
    }

    pub fn matches_tags(&self, required: &[String]) -> bool {
        required
            .iter()
            .all(|tag| self.tags.iter().any(|item| item == tag))
    }

    pub fn add_tag(&mut self, tag: &str) {
        let tag = tag.trim().to_ascii_lowercase();
        if !tag.is_empty() && !self.tags.iter().any(|item| item == &tag) {
            self.tags.push(tag);
            self.tags.sort();
        }
    }

    pub fn remove_tag(&mut self, tag: &str) {
        let tag = tag.trim().to_ascii_lowercase();
        self.tags.retain(|item| item != &tag);
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    pub info: SongInfo,
    #[allow(dead_code)]
    pub directory: PathBuf,
    pub audio: PathBuf,
    #[allow(dead_code)]
    pub image: Option<PathBuf>,
    #[allow(dead_code)]
    pub lyrics: Option<PathBuf>,
}

impl Song {
    pub fn from_directory(directory: PathBuf) -> Result<Self, LibraryError> {
        let id = directory
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| LibraryError::Message("song directory has no valid id".to_string()))?;
        let info_path = directory.join("info.json");
        let json = fs::read_to_string(&info_path).map_err(|source| LibraryError::Read {
            path: info_path.clone(),
            source,
        })?;
        let info: SongInfo = serde_json::from_str(&json).map_err(|source| LibraryError::Json {
            path: info_path,
            source,
        })?;
        let info = info.normalized(id);
        let audio = directory.join("song.mp3");
        if !audio.is_file() {
            return Err(LibraryError::Message(format!(
                "{} has no song.mp3",
                directory.display()
            )));
        }
        let image = resolve_optional_file(
            &directory,
            &info.image,
            &["image.png", "image.jpg", "image.jpeg", "image.webp"],
        );
        let lyrics = resolve_optional_file(&directory, &info.lyrics, &["lyrics.txt"]);
        Ok(Self {
            info,
            directory,
            audio,
            image,
            lyrics,
        })
    }

    pub fn refresh_optional_files(&mut self) {
        self.image = resolve_optional_file(
            &self.directory,
            &self.info.image,
            &["image.png", "image.jpg", "image.jpeg", "image.webp"],
        );
        self.lyrics = resolve_optional_file(&self.directory, &self.info.lyrics, &["lyrics.txt"]);
    }
}

pub fn scan(paths: &AppPaths) -> Result<Vec<Song>, LibraryError> {
    if !paths.songs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut songs = Vec::new();
    let entries = fs::read_dir(&paths.songs_dir).map_err(|source| LibraryError::Read {
        path: paths.songs_dir.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LibraryError::Read {
            path: paths.songs_dir.clone(),
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match Song::from_directory(path.clone()) {
            Ok(song) => songs.push(song),
            Err(error) => eprintln!("groov: skipping {}: {error}", path.display()),
        }
    }
    songs.sort_by(|a, b| a.info.id.cmp(&b.info.id));
    Ok(songs)
}

pub fn next_id(paths: &AppPaths) -> Result<String, LibraryError> {
    let mut max_id = 0_u64;
    if paths.songs_dir.exists() {
        for entry in fs::read_dir(&paths.songs_dir).map_err(|source| LibraryError::Read {
            path: paths.songs_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| LibraryError::Read {
                path: paths.songs_dir.clone(),
                source,
            })?;
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(id) = name.parse::<u64>() {
                    max_id = max_id.max(id);
                }
            }
        }
    }
    Ok(max_id.saturating_add(1).to_string())
}

pub fn write_info_atomic(path: &Path, info: &SongInfo) -> Result<(), LibraryError> {
    let json = serde_json::to_string_pretty(info)
        .map_err(|source| LibraryError::Message(source.to_string()))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{json}\n")).map_err(|source| LibraryError::Write {
        path: temporary.clone(),
        source,
    })?;
    if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(LibraryError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

pub fn load_song(paths: &AppPaths, id: &str) -> Result<Song, LibraryError> {
    let directory = paths.song_dir(id);
    if !directory.is_dir() {
        return Err(LibraryError::Message(format!(
            "song id does not exist: {id}"
        )));
    }
    Song::from_directory(directory)
}

fn resolve_optional_file(directory: &Path, configured: &str, defaults: &[&str]) -> Option<PathBuf> {
    if !configured.is_empty() {
        let path = directory.join(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    defaults
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::{scan, SongInfo};
    use crate::paths::AppPaths;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scans_and_normalizes_metadata() {
        let temp = tempdir().unwrap();
        let paths = AppPaths {
            config_dir: temp.path().join("config"),
            settings_file: temp.path().join("config/settings.json"),
            data_dir: temp.path().join("data"),
            songs_dir: temp.path().join("data/songs"),
            cache_dir: temp.path().join("cache"),
        };
        let song_dir = paths.song_dir("13");
        fs::create_dir_all(&song_dir).unwrap();
        fs::write(song_dir.join("song.mp3"), b"audio").unwrap();
        let info = SongInfo {
            id: String::new(),
            artist: "A".into(),
            lyrics: String::new(),
            title: "T".into(),
            album: String::new(),
            description: String::new(),
            image: String::new(),
            tags: vec!["Rock".into(), "rock".into()],
            duration_seconds: 0,
            added_at: String::new(),
        };
        super::write_info_atomic(&song_dir.join("info.json"), &info).unwrap();
        let songs = scan(&paths).unwrap();
        assert_eq!(songs[0].info.id, "13");
        assert_eq!(songs[0].info.tags, vec!["rock"]);
    }

    #[test]
    fn tag_mutations_are_canonical() {
        let mut info = SongInfo {
            id: "1".into(),
            artist: "A".into(),
            lyrics: String::new(),
            title: "T".into(),
            album: String::new(),
            description: String::new(),
            image: String::new(),
            tags: vec!["rock".into()],
            duration_seconds: 0,
            added_at: String::new(),
        };
        info.add_tag(" Rock ");
        info.add_tag("LIVE");
        info.remove_tag("rock");
        assert_eq!(info.tags, vec!["live"]);
    }

    #[test]
    fn info_writes_atomically() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("info.json");
        let info = SongInfo {
            id: "7".into(),
            artist: "A".into(),
            lyrics: String::new(),
            title: "T".into(),
            album: String::new(),
            description: String::new(),
            image: String::new(),
            tags: vec!["x".into()],
            duration_seconds: 0,
            added_at: String::new(),
        };
        super::write_info_atomic(&path, &info).unwrap();
        assert!(path.is_file());
        assert!(!temp.path().join("info.json.tmp").exists());
        assert_eq!(
            serde_json::from_str::<SongInfo>(&fs::read_to_string(path).unwrap()).unwrap(),
            info
        );
    }
}
