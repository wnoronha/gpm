use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

use crate::errors::{GpmError, Result};
use crate::extractor::Extractor;
use crate::network::HttpClient;
use crate::paths::GpmPaths;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

#[async_trait]
pub trait Installer: Send + Sync {
    async fn install_and_discover(
        &self,
        repo: &str,
        version: &str,
        asset_url: &str,
        asset_name: &str,
        checksum_url: Option<&str>,
        checksum_name: Option<&str>,
    ) -> Result<Vec<PathBuf>>;
    fn link(&self, name: &str, version: &str, files: &[PathBuf]) -> Result<()>;
    fn unlink(&self, name: &str, files: &[PathBuf]) -> Result<()>;
    fn uninstall_version(&self, name: &str, version: &str, files: &[PathBuf]) -> Result<()>;
}

pub struct GpmInstaller<'a> {
    http: &'a dyn HttpClient,
    extractor: &'a dyn Extractor,
    paths: GpmPaths,
}

impl<'a> GpmInstaller<'a> {
    pub fn new(http: &'a dyn HttpClient, extractor: &'a dyn Extractor, paths: GpmPaths) -> Self {
        Self {
            http,
            extractor,
            paths,
        }
    }

    fn verify_checksum(
        &self,
        file_path: &Path,
        asset_name: &str,
        checksum_content: &str,
    ) -> Result<()> {
        let mut file = fs::File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash);

        // Parse checksum file. It could be:
        // 1. Just the hash (if the checksum file was for this specific asset)
        // 2. Lines of "hash  filename"
        for line in checksum_content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            if parts.len() == 1 {
                // If it's a single hash, assume it's for our asset
                if parts[0].to_lowercase() == hash_hex {
                    return Ok(());
                }
            } else {
                // Check if the filename matches
                let filename = parts[1].trim_start_matches('*');
                if filename == asset_name {
                    if parts[0].to_lowercase() == hash_hex {
                        return Ok(());
                    } else {
                        return Err(GpmError::Unknown(format!(
                            "Checksum mismatch for {}: expected {}, got {}",
                            asset_name, parts[0], hash_hex
                        )));
                    }
                }
            }
        }

        // If we got here and it was a single-line file that didn't match, or we didn't find the filename
        if checksum_content.lines().count() == 1 && checksum_content.split_whitespace().count() == 1
        {
            return Err(GpmError::Unknown(format!(
                "Checksum mismatch for {}: got {}",
                asset_name, hash_hex
            )));
        }

        // If it was a multi-line file and we didn't find the filename, we can't verify
        tracing::warn!(
            "Could not find checksum for {} in checksum file",
            asset_name
        );
        Ok(())
    }
}

#[async_trait]
impl<'a> Installer for GpmInstaller<'a> {
    async fn install_and_discover(
        &self,
        repo: &str,
        version: &str,
        asset_url: &str,
        asset_name: &str,
        checksum_url: Option<&str>,
        checksum_name: Option<&str>,
    ) -> Result<Vec<PathBuf>> {
        let package_name = repo
            .split('/')
            .next_back()
            .ok_or_else(|| GpmError::Unknown(format!("Invalid repo: {}", repo)))?;
        let version_dir = self
            .paths
            .ensure_cache_dir()?
            .join(package_name)
            .join(version);
        fs::create_dir_all(&version_dir)?;

        let tmp_dir = tempdir()?;
        let download_path = tmp_dir.path().join(asset_name);

        self.http.download_file(asset_url, &download_path).await?;

        // Verify checksum if available
        if let (Some(url), Some(name)) = (checksum_url, checksum_name) {
            println!("Verifying checksum...");
            let checksum_path = tmp_dir.path().join(name);
            self.http.download_file(url, &checksum_path).await?;
            let content = fs::read_to_string(&checksum_path)?;
            self.verify_checksum(&download_path, asset_name, &content)?;
        }

        if self.extractor.extract(&download_path, &version_dir).is_ok() {
            let binaries = self.extractor.find_binaries(&version_dir)?;
            Ok(binaries)
        } else {
            // Assume the downloaded file IS the binary
            let dest_path = version_dir.join(asset_name);
            if let Err(e) = fs::rename(&download_path, &dest_path) {
                if e.raw_os_error() == Some(18) {
                    fs::copy(&download_path, &dest_path)?;
                    let _ = fs::remove_file(&download_path);
                } else {
                    return Err(e.into());
                }
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms)?;
            }

            Ok(vec![dest_path])
        }
    }

    fn link(&self, _name: &str, _version: &str, files: &[PathBuf]) -> Result<()> {
        let bin_dir = self.paths.ensure_bin_dir()?;
        let cache_dir = self.paths.cache_dir().canonicalize()?;

        for src_path in files {
            let src_path_canon = src_path.canonicalize()?;
            if !src_path_canon.starts_with(&cache_dir) {
                return Err(GpmError::Unknown(format!(
                    "Security violation: Link source {:?} is outside cache directory.",
                    src_path
                )));
            }

            let file_name = src_path
                .file_name()
                .ok_or_else(|| GpmError::Unknown(format!("Invalid file path: {:?}", src_path)))?;
            let dest_path = bin_dir.join(file_name);

            if dest_path.exists() || dest_path.is_symlink() {
                if dest_path.is_dir() && !dest_path.is_symlink() {
                    fs::remove_dir_all(&dest_path)?;
                } else {
                    fs::remove_file(&dest_path)?;
                }
            }

            symlink(src_path, &dest_path)?;
            println!("Linked {:?} to {:?}", file_name, bin_dir);
        }

        Ok(())
    }

    fn unlink(&self, _name: &str, files: &[PathBuf]) -> Result<()> {
        let bin_dir = self.paths.bin_dir();
        for path in files {
            let file_name = path
                .file_name()
                .ok_or_else(|| GpmError::Unknown(format!("Invalid file path: {:?}", path)))?;
            let dest_path = bin_dir.join(file_name);
            if dest_path.is_symlink() {
                fs::remove_file(&dest_path)?;
                println!("Unlinked {:?}", file_name);
            } else if dest_path.exists() {
                tracing::warn!("{:?} is not a symlink, skipping.", dest_path);
            }
        }
        Ok(())
    }

    fn uninstall_version(
        &self,
        package_name: &str,
        version: &str,
        files: &[PathBuf],
    ) -> Result<()> {
        for file_path in files {
            if file_path.exists() {
                fs::remove_file(file_path)?;
            }
        }

        let version_dir = self.paths.cache_dir().join(package_name).join(version);
        if version_dir.exists() && fs::read_dir(&version_dir)?.next().is_none() {
            fs::remove_dir(&version_dir)?;
        }

        let package_dir = self.paths.cache_dir().join(package_name);
        if package_dir.exists() && fs::read_dir(&package_dir)?.next().is_none() {
            fs::remove_dir(&package_dir)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::Extractor;
    use crate::network::HttpClient;
    use async_trait::async_trait;
    use serde_json::Value;
    use std::io::Write;
    use tempfile::tempdir;

    struct MockHttpClient;
    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn fetch_json(&self, _url: &str) -> Result<Value> {
            unimplemented!()
        }
        async fn download_file(&self, _url: &str, _dest: &Path) -> Result<()> {
            unimplemented!()
        }
    }

    struct MockExtractor;
    impl Extractor for MockExtractor {
        fn extract(&self, _archive_path: &Path, _dest: &Path) -> Result<()> {
            unimplemented!()
        }
        fn find_binaries(&self, _dir: &Path) -> Result<Vec<PathBuf>> {
            unimplemented!()
        }
        fn is_executable(&self, _path: &Path) -> Result<bool> {
            unimplemented!()
        }
    }

    #[test]
    fn test_verify_checksum_single_line() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"; // sha256 of "hello world"

        let http = MockHttpClient;
        let extractor = MockExtractor;
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(&http, &extractor, paths);

        installer
            .verify_checksum(&file_path, "test.bin", hash)
            .unwrap();
    }

    #[test]
    fn test_verify_checksum_multi_line() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let checksum_content = format!("{}  test.bin\notherhash  other.bin", hash);

        let http = MockHttpClient;
        let extractor = MockExtractor;
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(&http, &extractor, paths);

        installer
            .verify_checksum(&file_path, "test.bin", &checksum_content)
            .unwrap();
    }

    #[test]
    fn test_verify_checksum_mismatch() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "wronghash";

        let http = MockHttpClient;
        let extractor = MockExtractor;
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(&http, &extractor, paths);

        assert!(
            installer
                .verify_checksum(&file_path, "test.bin", hash)
                .is_err()
        );
    }
}

fn symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)?;
    }
    #[cfg(windows)]
    {
        if src.is_dir() {
            std::os::windows::fs::symlink_dir(src, dst)?;
        } else {
            std::os::windows::fs::symlink_file(src, dst)?;
        }
    }
    Ok(())
}
