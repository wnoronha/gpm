#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    use gpm::cli::{InstallArgs, LinkArgs, UninstallArgs, UnlinkArgs};
    use gpm::commands;
    use gpm::extractor::ArchiveExtractor;
    use gpm::github::{Asset, Release};
    use gpm::installer::GpmInstaller;
    use gpm::manifest::JsonStateManager;
    use gpm::paths::GpmPaths;

    mockall::mock! {
        pub HttpClient {}
        #[async_trait::async_trait]
        impl gpm::network::HttpClient for HttpClient {
            async fn fetch_json(&self, url: &str) -> gpm::errors::Result<serde_json::Value>;
            async fn download_file(&self, url: &str, dest: &std::path::Path) -> gpm::errors::Result<()>;
        }
    }

    mockall::mock! {
        pub ReleaseFetcher {}
        #[async_trait::async_trait]
        impl gpm::github::ReleaseFetcher for ReleaseFetcher {
            async fn get_releases(&self, repo: &str) -> gpm::errors::Result<Vec<Release>>;
            async fn get_release_by_tag(&self, repo: &str, tag: &str) -> gpm::errors::Result<Release>;
        }
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());

        let mut http = MockHttpClient::new();
        http.expect_fetch_json().returning(|_| Ok(json!([])));
        http.expect_download_file().returning(|_, dest| {
            fs::write(dest, b"\x7fELFfakebinary")?;
            Ok(())
        });

        let mut github = MockReleaseFetcher::new();
        github.expect_get_releases().returning(|_| {
            Ok(vec![Release {
                tag_name: "v1.0".to_string(),
                published_at: chrono::Utc::now(),
                assets: vec![Asset {
                    name: "test-bin".to_string(),
                    browser_download_url: "http://example.com/v1.0/test-bin".to_string(),
                    size: 100,
                }],
            }])
        });
        github.expect_get_release_by_tag().returning(|_, tag| {
            Ok(Release {
                tag_name: tag.to_string(),
                published_at: chrono::Utc::now(),
                assets: vec![Asset {
                    name: "test-bin".to_string(),
                    browser_download_url: format!("http://example.com/{}/test-bin", tag),
                    size: 100,
                }],
            })
        });

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
