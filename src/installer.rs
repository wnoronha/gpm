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

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Installer: Send + Sync {
    async fn install_and_discover<'a>(
        &'a self,
        repo: &'a str,
        version: &'a str,
        asset_url: &'a str,
        asset_name: &'a str,
        checksum_url: Option<&'a str>,
        checksum_name: Option<&'a str>,
    ) -> Result<Vec<PathBuf>>;
    fn link(&self, name: &str, version: &str, files: &[PathBuf]) -> Result<()>;
    fn unlink(&self, name: &str, files: &[PathBuf]) -> Result<()>;
    fn uninstall_version(&self, name: &str, version: &str, files: &[PathBuf]) -> Result<()>;
}

pub struct GpmInstaller {
    http: std::sync::Arc<dyn HttpClient>,
    extractor: std::sync::Arc<dyn Extractor>,
    paths: GpmPaths,
}

impl GpmInstaller {
    pub fn new(
        http: std::sync::Arc<dyn HttpClient>,
        extractor: std::sync::Arc<dyn Extractor>,
        paths: GpmPaths,
    ) -> Self {
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
        // 2. Lines of "hash  filename" (separated by whitespace, potentially containing spaces or relative paths)
        for line in checksum_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let mut parts = line.splitn(2, |c: char| c.is_whitespace());
            let hash_val = match parts.next() {
                Some(h) => h,
                None => continue,
            };

            let remaining = parts.next().unwrap_or("").trim_start();
            if remaining.is_empty() {
                if hash_val.to_lowercase() == hash_hex {
                    return Ok(());
                }
            } else {
                let cleaned_path = remaining.trim_start_matches('*');
                let filename = Path::new(cleaned_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(cleaned_path);

                if filename == asset_name {
                    if hash_val.to_lowercase() == hash_hex {
                        return Ok(());
                    } else {
                        return Err(GpmError::Unknown(format!(
                            "Checksum mismatch for {}: expected {}, got {}",
                            asset_name, hash_val, hash_hex
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
impl Installer for GpmInstaller {
    async fn install_and_discover<'a>(
        &'a self,
        repo: &'a str,
        version: &'a str,
        asset_url: &'a str,
        asset_name: &'a str,
        checksum_url: Option<&'a str>,
        checksum_name: Option<&'a str>,
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for bin in &binaries {
                    let mut perms = fs::metadata(bin)?.permissions();
                    if perms.mode() & 0o111 == 0 {
                        perms.set_mode(perms.mode() | 0o111);
                        fs::set_permissions(bin, perms)?;
                    }
                }
            }
            Ok(binaries)
        } else {
            // Assume the downloaded file IS the binary
            let dest_path = version_dir.join(asset_name);
            if let Err(e) = fs::rename(&download_path, &dest_path) {
                let is_cross_device = e.kind() == std::io::ErrorKind::CrossesDevices
                    || e.raw_os_error() == Some(18) // EXDEV (Unix)
                    || e.raw_os_error() == Some(17); // ERROR_NOT_SAME_DEVICE (Windows)

                if is_cross_device {
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
            if !src_path.exists() {
                return Err(GpmError::Unknown(format!(
                    "Cannot link missing file: {:?}. The cached binary might have been deleted. Try re-running gpm install with the --version flag.",
                    src_path
                )));
            }
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
    use crate::extractor::MockExtractor;
    use crate::network::MockHttpClient;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_verify_checksum_single_line() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"; // sha256 of "hello world" (no newline)

        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(http, extractor, paths);

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

        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(http, extractor, paths);

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

        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(http, extractor, paths);

        assert!(
            installer
                .verify_checksum(&file_path, "test.bin", hash)
                .is_err()
        );
    }

    #[test]
    fn test_verify_checksum_filename_with_spaces() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("my cool asset.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let checksum_content = format!("{}  my cool asset.bin\notherhash  other.bin", hash);

        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(http, extractor, paths);

        installer
            .verify_checksum(&file_path, "my cool asset.bin", &checksum_content)
            .unwrap();
    }

    #[test]
    fn test_verify_checksum_with_relative_paths() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("asset.bin");
        let mut f = fs::File::create(&file_path).unwrap();
        f.write_all(b"hello world").unwrap();

        let hash = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let checksum_content = format!("{}  ./build/dist/asset.bin\notherhash  other.bin", hash);

        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());
        let installer = GpmInstaller::new(http, extractor, paths);

        installer
            .verify_checksum(&file_path, "asset.bin", &checksum_content)
            .unwrap();
    }

    #[test]
    fn test_link_missing_file_returns_error() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let installer = GpmInstaller::new(http, extractor, paths);

        let missing_file = temp.path().join("does_not_exist");
        let result = installer.link("test_pkg", "v1.0", &[missing_file]);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Cannot link missing file"),
            "Got err: {}",
            err_msg
        );
    }

    #[test]
    fn test_link_creates_symlink() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let installer = GpmInstaller::new(http, extractor, paths.clone());

        let cache_file = paths.cache_dir().join("my_bin");
        fs::write(&cache_file, b"test").unwrap();

        installer.link("test_pkg", "v1.0", &[cache_file]).unwrap();

        let bin_link = paths.bin_dir().join("my_bin");
        assert!(bin_link.exists());
        
        #[cfg(unix)]
        {
            assert!(fs::symlink_metadata(&bin_link).unwrap().is_symlink());
            assert_eq!(fs::read_link(&bin_link).unwrap(), paths.cache_dir().canonicalize().unwrap().join("my_bin"));
        }
    }

    #[test]
    fn test_link_replaces_existing_symlink_or_file() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let bin_file = paths.bin_dir().join("my_bin");
        fs::write(&bin_file, b"old").unwrap();

        let installer = GpmInstaller::new(http, extractor, paths.clone());

        let cache_file = paths.cache_dir().join("my_bin");
        fs::write(&cache_file, b"new").unwrap();

        installer.link("test_pkg", "v1.0", &[cache_file]).unwrap();

        assert!(bin_file.exists());
        assert_eq!(fs::read_to_string(&bin_file).unwrap(), "new");
        
        #[cfg(unix)]
        {
            assert!(fs::symlink_metadata(&bin_file).unwrap().is_symlink());
        }
    }

    #[test]
    fn test_link_security_violation() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let installer = GpmInstaller::new(http, extractor, paths.clone());

        let outside_file = temp.path().join("outside");
        fs::write(&outside_file, b"bad").unwrap();

        let res = installer.link("test_pkg", "v1.0", &[outside_file]);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Security violation"));
    }

    #[test]
    fn test_unlink_removes_symlink() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let cache_file = paths.cache_dir().join("my_bin");
        fs::write(&cache_file, b"test").unwrap();

        let installer = GpmInstaller::new(http, extractor, paths.clone());
        installer.link("test_pkg", "v1.0", &[cache_file.clone()]).unwrap();
        
        let bin_file = paths.bin_dir().join("my_bin");
        assert!(bin_file.exists());

        installer.unlink("test_pkg", &[cache_file.clone()]).unwrap();

        assert!(!bin_file.exists());
    }

    #[test]
    fn test_unlink_noop_when_absent_or_file() {
        let temp = tempdir().unwrap();
        let http = std::sync::Arc::new(MockHttpClient::new());
        let extractor = std::sync::Arc::new(MockExtractor::new());
        let paths = GpmPaths::with_home(temp.path());

        fs::create_dir_all(paths.cache_dir()).unwrap();
        fs::create_dir_all(paths.bin_dir()).unwrap();

        let cache_file = paths.cache_dir().join("my_bin");
        fs::write(&cache_file, b"test").unwrap();
        
        let bin_file = paths.bin_dir().join("my_bin");
        fs::write(&bin_file, b"regular").unwrap();

        let installer = GpmInstaller::new(http, extractor, paths.clone());
        
        // This shouldn't error, and shouldn't remove the regular file
        installer.unlink("test_pkg", &[cache_file.clone()]).unwrap();

        assert!(bin_file.exists());
        assert_eq!(fs::read_to_string(&bin_file).unwrap(), "regular");
        
        // Also test when it doesn't exist at all
        fs::remove_file(&bin_file).unwrap();
        installer.unlink("test_pkg", &[cache_file]).unwrap();
    }
}

fn symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)?;
    }
    #[cfg(windows)]
    {
        let res = if src.is_dir() {
            std::os::windows::fs::symlink_dir(src, dst)
        } else {
            std::os::windows::fs::symlink_file(src, dst)
        };

        if let Err(e) = res {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                tracing::warn!(
                    "Symlink permission denied on Windows. Falling back to copy. Source: {:?}, Destination: {:?}",
                    src,
                    dst
                );
                if src.is_dir() {
                    return Err(e.into());
                } else {
                    std::fs::copy(src, dst)?;
                }
            } else {
                return Err(e.into());
            }
        }
    }
    Ok(())
}
