use crate::errors::{GpmError, Result};
use crate::paths::GpmPaths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Package {
    pub repo: String,
    pub active_version: Option<String>,
    pub versions: HashMap<String, VersionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionInfo {
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub bins: Vec<String>,
}

pub trait StateManager: Send + Sync {
    fn get_packages(&self) -> Result<HashMap<String, Package>>;
    fn get_package(&self, name: &str) -> Result<Option<Package>>;
    #[allow(clippy::too_many_arguments)]
    fn add_package(
        &self,
        name: &str,
        repo: &str,
        version: &str,
        files: &[PathBuf],
        bins: &[String],
        bin: Option<&str>,
        link: Option<&str>,
    ) -> Result<()>;
    fn remove_package(&self, name: &str, version: Option<&str>) -> Result<Option<Package>>;
    fn set_active_version(&self, name: &str, version: Option<&str>) -> Result<()>;
}

pub struct JsonStateManager {
    paths: GpmPaths,
}

impl JsonStateManager {
    pub fn new(paths: GpmPaths) -> Self {
        Self { paths }
    }

    fn get_receipt_path(&self, name: &str) -> PathBuf {
        self.paths
            .config_dir()
            .join("receipts")
            .join(format!("{}.json", name))
    }

    fn load_receipt(&self, name: &str) -> Result<Option<Package>> {
        let path = self.get_receipt_path(name);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        match serde_json::from_str(&content) {
            Ok(pkg) => Ok(Some(pkg)),
            Err(e) => {
                tracing::warn!("Failed to parse receipt at {:?}: {}", path, e);
                Ok(None)
            }
        }
    }

    fn save_receipt(&self, name: &str, pkg: &Package) -> Result<()> {
        let receipts_dir = self.paths.config_dir().join("receipts");
        fs::create_dir_all(&receipts_dir)?;
        let path = receipts_dir.join(format!("{}.json", name));
        let content = serde_json::to_string_pretty(pkg)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn delete_receipt(&self, name: &str) -> Result<()> {
        let path = self.get_receipt_path(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

impl StateManager for JsonStateManager {
    fn get_packages(&self) -> Result<HashMap<String, Package>> {
        let mut packages = HashMap::new();
        let receipts_dir = self.paths.config_dir().join("receipts");

        if !receipts_dir.exists() {
            return Ok(packages);
        }

        for entry in fs::read_dir(receipts_dir)? {
            let entry = entry?;
            let path = entry.path();

            let is_json = path.extension().and_then(|s| s.to_str()) == Some("json");
            if !is_json {
                continue;
            }

            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };

            if let Ok(Some(pkg)) = self.load_receipt(name) {
                packages.insert(name.to_string(), pkg);
            }
        }
        Ok(packages)
    }

    fn get_package(&self, name: &str) -> Result<Option<Package>> {
        self.load_receipt(name)
    }

    fn add_package(
        &self,
        name: &str,
        repo: &str,
        version: &str,
        files: &[PathBuf],
        bins: &[String],
        bin: Option<&str>,
        link: Option<&str>,
    ) -> Result<()> {
        let mut pkg = self.load_receipt(name)?.unwrap_or_else(|| Package {
            repo: repo.to_string(),
            active_version: None,
            versions: HashMap::new(),
            bin: None,
            link: None,
        });

        pkg.versions.insert(
            version.to_string(),
            VersionInfo {
                files: files.to_vec(),
                bins: bins.to_vec(),
            },
        );
        // Flags describe this install: explicit values (or None) win.
        pkg.bin = bin.map(|s| s.to_string());
        pkg.link = link.map(|s| s.to_string());
        pkg.active_version = Some(version.to_string());

        self.save_receipt(name, &pkg)
    }

    fn remove_package(&self, name: &str, version: Option<&str>) -> Result<Option<Package>> {
        let mut pkg = match self.load_receipt(name)? {
            Some(p) => p,
            None => return Ok(None),
        };

        if let Some(v) = version {
            pkg.versions.remove(v);
            if pkg.active_version.as_deref() == Some(v) {
                pkg.active_version = None;
            }

            if pkg.versions.is_empty() {
                self.delete_receipt(name)?;
                Ok(Some(pkg))
            } else {
                self.save_receipt(name, &pkg)?;
                Ok(Some(pkg))
            }
        } else {
            self.delete_receipt(name)?;
            Ok(Some(pkg))
        }
    }

    fn set_active_version(&self, name: &str, version: Option<&str>) -> Result<()> {
        let mut pkg = self
            .load_receipt(name)?
            .ok_or_else(|| GpmError::PackageNotFoundError(name.to_string()))?;

        if let Some(v) = version {
            if !pkg.versions.contains_key(v) {
                return Err(GpmError::PackageNotFoundError(format!(
                    "Version {} not found for {}",
                    v, name
                )));
            }
            pkg.active_version = Some(v.to_string());
        } else {
            pkg.active_version = None;
        }

        self.save_receipt(name, &pkg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_manifest_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());

        let manager = JsonStateManager::new(paths);
        let name = "ripgrep";
        let repo = "BurntSushi/ripgrep";
        let version = "14.1.0";
        let files = vec![PathBuf::from("rg")];

        // Add
        manager
            .add_package(name, repo, version, &files, &[], None, None)
            .unwrap();

        // Get
        let pkg = manager.get_package(name).unwrap().unwrap();
        assert_eq!(pkg.repo, repo);
        assert_eq!(pkg.active_version, Some(version.to_string()));
        assert!(pkg.versions.contains_key(version));

        // List
        let packages = manager.get_packages().unwrap();
        assert!(packages.contains_key(name));

        // Set active
        manager.set_active_version(name, None).unwrap();
        let pkg = manager.get_package(name).unwrap().unwrap();
        assert_eq!(pkg.active_version, None);

        // Remove
        manager.remove_package(name, None).unwrap();
        assert!(manager.get_package(name).unwrap().is_none());
    }

    #[test]
    fn test_corrupted_receipt_warns() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());
        let manager = JsonStateManager::new(paths);

        let receipts_dir = manager.paths.config_dir().join("receipts");
        fs::create_dir_all(&receipts_dir).unwrap();
        let path = receipts_dir.join("corrupted.json");
        fs::write(&path, "{ invalid json }").unwrap();

        let _pkg = manager.get_package("corrupted").unwrap();
    }

    #[test]
    fn test_add_package_records_bins_and_selectors() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());
        let manager = JsonStateManager::new(paths);

        let bins = vec!["mytool".to_string()];
        manager
            .add_package(
                "mytool",
                "owner/mytool",
                "2.0",
                &[PathBuf::from("client")],
                &bins,
                Some("client"),
                None,
            )
            .unwrap();

        let pkg = manager.get_package("mytool").unwrap().unwrap();
        assert_eq!(pkg.bin, Some("client".to_string()));
        assert_eq!(pkg.link, None);
        assert_eq!(pkg.versions["2.0"].bins, bins);
    }

    #[test]
    fn test_legacy_receipt_without_new_fields_loads() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());
        let manager = JsonStateManager::new(paths);

        let receipts_dir = manager.paths.config_dir().join("receipts");
        fs::create_dir_all(&receipts_dir).unwrap();
        fs::write(
            receipts_dir.join("legacy.json"),
            r#"{"repo":"owner/legacy","active_version":"1.0","versions":{"1.0":{"files":["/cache/legacy/1.0/legacy-x64"]}}}"#,
        )
        .unwrap();

        let pkg = manager.get_package("legacy").unwrap().unwrap();
        assert_eq!(pkg.bin, None);
        assert_eq!(pkg.link, None);
        assert!(pkg.versions["1.0"].bins.is_empty());
    }
}
