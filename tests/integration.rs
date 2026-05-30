#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::fs;
    use tempfile::tempdir;

    use gpm::cli::{InstallArgs, LinkArgs, UninstallArgs, UnlinkArgs};
    use gpm::commands;
    use gpm::errors::Result;
    use gpm::extractor::ArchiveExtractor;
    use gpm::github::{Asset, Release, ReleaseFetcher};
    use gpm::installer::GpmInstaller;
    use gpm::manifest::JsonStateManager;
    use gpm::network::HttpClient;
    use gpm::paths::GpmPaths;

    struct MockHttpClient;

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn fetch_json(&self, _url: &str) -> Result<Value> {
            Ok(json!([]))
        }
        async fn download_file(&self, _url: &str, dest: &std::path::Path) -> Result<()> {
            fs::write(dest, b"\x7fELFfakebinary")?;
            Ok(())
        }
    }

    struct MockGithubClient;

    #[async_trait]
    impl ReleaseFetcher for MockGithubClient {
        async fn get_releases(&self, _repo: &str) -> Result<Vec<Release>> {
            Ok(vec![Release {
                tag_name: "v1.0".to_string(),
                published_at: chrono::Utc::now(),
                assets: vec![Asset {
                    name: "test-bin".to_string(),
                    browser_download_url: "http://example.com/v1.0/test-bin".to_string(),
                    size: 100,
                }],
            }])
        }
        async fn get_release_by_tag(&self, _repo: &str, tag: &str) -> Result<Release> {
            Ok(Release {
                tag_name: tag.to_string(),
                published_at: chrono::Utc::now(),
                assets: vec![Asset {
                    name: "test-bin".to_string(),
                    browser_download_url: format!("http://example.com/{}/test-bin", tag),
                    size: 100,
                }],
            })
        }
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());

        let http = MockHttpClient;
        let github = MockGithubClient;
        let extractor = ArchiveExtractor::new();
        let installer = GpmInstaller::new(&http, &extractor, paths.clone());
        let state = JsonStateManager::new(paths.clone());

        let repo = "owner/testpkg";
        let name = "testpkg";

        // 1. Install v1.0
        let install_args = InstallArgs {
            repo: repo.to_string(),
            version: Some("v1.0".to_string()),
            min_age: None,
            pattern: None,
        };
        commands::install(&installer, &github, &state, &install_args)
            .await
            .unwrap();

        let bin_dir = paths.bin_dir();
        let binary_path = bin_dir.join("test-bin");
        let cached_v1 = paths.cache_dir().join(name).join("v1.0").join("test-bin");

        assert!(binary_path.exists());
        assert!(cached_v1.exists());

        // 2. Install v2.0
        let install_args_v2 = InstallArgs {
            repo: repo.to_string(),
            version: Some("v2.0".to_string()),
            min_age: None,
            pattern: None,
        };
        commands::install(&installer, &github, &state, &install_args_v2)
            .await
            .unwrap();

        let cached_v2 = paths.cache_dir().join(name).join("v2.0").join("test-bin");
        assert!(cached_v2.exists());
        // Verify link points to v2.0
        assert_eq!(fs::read_link(&binary_path).unwrap(), cached_v2);

        // 3. Link v1.0
        let link_args = LinkArgs {
            name: name.to_string(),
            version: "v1.0".to_string(),
        };
        commands::link(&installer, &state, &link_args).unwrap();
        assert_eq!(fs::read_link(&binary_path).unwrap(), cached_v1);

        // 4. Unlink
        let unlink_args = UnlinkArgs {
            name: name.to_string(),
        };
        commands::unlink(&installer, &state, &unlink_args).unwrap();
        assert!(!binary_path.exists());

        // 5. Uninstall
        let uninstall_args = UninstallArgs {
            name: name.to_string(),
            pkg_version: None,
        };
        commands::uninstall(&installer, &state, &uninstall_args).unwrap();
        assert!(!paths.cache_dir().join(name).exists());
    }
}
