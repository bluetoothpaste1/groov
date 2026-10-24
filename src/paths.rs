use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathsError {
    #[error("cannot determine home directory (set HOME)")]
    MissingHome,
    #[error("cannot create {path}: {source}")]
    Create {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub settings_file: PathBuf,
    pub data_dir: PathBuf,
    pub songs_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, PathsError> {
        let home = env::var_os("HOME").map(PathBuf::from);
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.clone().map(|path| path.join(".config")))
            .ok_or(PathsError::MissingHome)?;
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.clone().map(|path| path.join(".local").join("share")))
            .ok_or(PathsError::MissingHome)?;
        let cache_home = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|path| path.join(".cache")))
            .ok_or(PathsError::MissingHome)?;
        let config_dir = config_home.join("groov");
        let data_dir = data_home.join("groov");
        Ok(Self {
            settings_file: config_dir.join("settings.json"),
            songs_dir: data_dir.join("songs"),
            cache_dir: cache_home.join("groov"),
            config_dir,
            data_dir,
        })
    }

    pub fn ensure(&self) -> Result<(), PathsError> {
        for path in [
            &self.config_dir,
            &self.data_dir,
            &self.songs_dir,
            &self.cache_dir,
        ] {
            fs::create_dir_all(path).map_err(|source| PathsError::Create {
                path: path.clone(),
                source,
            })?;
        }
        Ok(())
    }

    pub fn song_dir(&self, id: &str) -> PathBuf {
        self.songs_dir.join(id)
    }

    pub fn describe(&self) -> String {
        format!(
            "config  {}\ndata    {}\nsongs   {}\ncache   {}",
            display(&self.config_dir),
            display(&self.data_dir),
            display(&self.songs_dir),
            display(&self.cache_dir)
        )
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::AppPaths;

    #[test]
    fn song_paths_are_stable() {
        let paths = AppPaths {
            config_dir: "/tmp/config/groov".into(),
            settings_file: "/tmp/config/groov/settings.json".into(),
            data_dir: "/tmp/data/groov".into(),
            songs_dir: "/tmp/data/groov/songs".into(),
            cache_dir: "/tmp/cache/groov".into(),
        };
        assert_eq!(
            paths.song_dir("13"),
            std::path::Path::new("/tmp/data/groov/songs/13")
        );
    }
}
