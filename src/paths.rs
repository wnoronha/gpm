use crate::errors::Result;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct GpmPaths {
    home_dir: PathBuf,
    is_custom: bool,
}

impl Default for GpmPaths {
    fn default() -> Self {
        Self::new()
    }
}

impl GpmPaths {
    pub fn new() -> Self {
        let (home_dir, is_custom) = if let Ok(gpm_home) = env::var("GPM_HOME") {
            (PathBuf::from(gpm_home), true)
        } else {
            (
                dirs::home_dir().expect("Could not determine home directory"),
                false,
            )
        };
        Self {
            home_dir,
            is_custom,
        }
    }

    pub fn with_home<P: AsRef<Path>>(home: P) -> Self {
        Self {
            home_dir: home.as_ref().to_path_buf(),
            is_custom: true,
        }
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn ensure_home_dir(&self) -> Result<PathBuf> {
        fs::create_dir_all(&self.home_dir)?;
        Ok(self.home_dir.clone())
    }

    pub fn config_dir(&self) -> PathBuf {
        if self.is_custom {
            return self.home_dir.join(".config").join("gpm");
        }
        dirs::config_dir()
            .map(|p| p.join("gpm"))
            .unwrap_or_else(|| self.home_dir.join(".config").join("gpm"))
    }

    pub fn ensure_config_dir(&self) -> Result<PathBuf> {
        let path = self.config_dir();
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn cache_dir(&self) -> PathBuf {
        if self.is_custom {
            return self.home_dir.join(".cache").join("gpm");
        }
        dirs::cache_dir()
            .map(|p| p.join("gpm"))
            .unwrap_or_else(|| self.home_dir.join(".cache").join("gpm"))
    }

    pub fn ensure_cache_dir(&self) -> Result<PathBuf> {
        let path = self.cache_dir();
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.home_dir.join(".local").join("bin")
    }

    pub fn ensure_bin_dir(&self) -> Result<PathBuf> {
        let path = self.bin_dir();
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_home_dir_default() {
        let paths = GpmPaths::new();
        let home = paths.home_dir();
        assert!(!home.as_os_str().is_empty());
    }

    #[test]
    fn test_get_home_dir_override() {
        let temp = tempdir().unwrap();
        let temp_path = temp.path();
        let paths = GpmPaths::with_home(temp_path);
        assert_eq!(paths.home_dir(), temp_path);
    }
}
