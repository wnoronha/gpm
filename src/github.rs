use crate::errors::{GpmError, Result};
use crate::network::HttpClient;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Release {
    pub tag_name: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ReleaseFetcher: Send + Sync {
    async fn get_releases(&self, repo: &str) -> Result<Vec<Release>>;
    async fn get_release_by_tag(&self, repo: &str, tag: &str) -> Result<Release>;
}

pub struct GithubClient<'a> {
    http: &'a dyn HttpClient,
}

impl<'a> GithubClient<'a> {
    pub fn new(http: &'a dyn HttpClient) -> Self {
        Self { http }
    }

    pub fn parse_age(age_str: &str) -> Result<Duration> {
        let age_str = age_str.to_lowercase();
        let value: i64 = age_str[..age_str.len() - 1]
            .parse()
            .map_err(|_| GpmError::Unknown(format!("Invalid age value: {}", age_str)))?;
        let unit = &age_str[age_str.len() - 1..];

        match unit {
            "h" => Ok(Duration::hours(value)),
            "d" => Ok(Duration::days(value)),
            "m" => Ok(Duration::days(value * 30)),
            _ => Err(GpmError::Unknown(format!("Invalid age unit: {}", unit))),
        }
    }

    pub fn get_valid_release(
        releases: Vec<Release>,
        min_age_str: Option<&str>,
    ) -> Result<Option<Release>> {
        if releases.is_empty() {
            return Ok(None);
        }

        let min_age = match min_age_str {
            Some(s) => Self::parse_age(s)?,
            None => {
                for release in &releases {
                    if !release.draft && !release.prerelease {
                        return Ok(Some(release.clone()));
                    }
                }
                return Ok(Some(releases[0].clone()));
            }
        };

        let now = Utc::now();
        for release in releases {
            if !release.draft && !release.prerelease && now - release.published_at >= min_age {
                return Ok(Some(release));
            }
        }

        Ok(None)
    }

    pub fn select_asset(
        assets: &[Asset],
        platform: &str,
        arch: &str,
        pattern: Option<&str>,
    ) -> (Option<Asset>, Option<Asset>) {
        let mut best_asset = None;
        let mut best_score = -1;

        let arch_aliases = get_arch_aliases(arch);

        for asset in assets {
            let name = asset.name.to_lowercase();

            // 1. Filter by user pattern
            if pattern.is_some_and(|p| !name.contains(&p.to_lowercase())) {
                continue;
            }

            // 2. Disqualify metadata (except for searching checksums)
            if [
                ".sha256",
                ".asc",
                ".sig",
                ".md5",
                ".txt",
                ".sha256sum",
                ".deb",
                ".rpm",
                ".msi",
            ]
            .iter()
            .any(|ext| name.contains(ext))
            {
                continue;
            }

            let mut score = 0;

            // 3. Platform Detection
            let is_windows = ["windows", "pc-windows", "win32", "win64"]
                .iter()
                .any(|k| name.contains(k))
                || name.ends_with(".exe");
            let is_macos = ["darwin", "macos", "apple-darwin", "osx"]
                .iter()
                .any(|k| name.contains(k));
            let is_linux = ["linux", "musl", "tux", "unknown-linux"]
                .iter()
                .any(|k| name.contains(k));

            match platform {
                "linux" => {
                    if is_windows || is_macos {
                        continue;
                    }
                    if is_linux {
                        score += 20;
                    } else {
                        score += 5;
                    }
                }
                "macos" | "darwin" => {
                    if is_windows || is_linux {
                        continue;
                    }
                    if is_macos {
                        score += 20;
                    } else {
                        score += 5;
                    }
                }
                "windows" => {
                    if is_macos || (is_linux && !name.contains("pc-windows")) {
                        continue;
                    }
                    if is_windows {
                        score += 20;
                    } else {
                        score += 5;
                    }
                }
                _ => {
                    score += 5;
                }
            }

            // 4. Architecture Matching
            let mut arch_match = false;
            for a in &arch_aliases {
                if name.contains(a) {
                    score += 10;
                    arch_match = true;
                    break;
                }
            }
            if !arch_match {
                score -= 5;
            }

            // 5. Extensions
            if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                score += 2;
            } else if name.ends_with(".zip") {
                score += 1;
            }

            if score > best_score {
                best_score = score;
                best_asset = Some(asset.clone());
            }
        }

        // Search for a matching checksum asset
        let checksum_asset = if let Some(ref asset) = best_asset {
            let asset_name = asset.name.to_lowercase();
            assets
                .iter()
                .find(|a| {
                    let a_name = a.name.to_lowercase();
                    (a_name.contains(&asset_name)
                        && (a_name.ends_with(".sha256") || a_name.ends_with(".sha256sum")))
                        || a_name == "checksums.txt"
                        || a_name == "sha256sums.txt"
                })
                .cloned()
        } else {
            None
        };

        (best_asset, checksum_asset)
    }
}

static ARCH_MAP: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert("x86_64", vec!["x86_64", "amd64", "x64"]);
    map.insert("arm64", vec!["arm64", "aarch64", "armv8"]);
    map.insert("i386", vec!["i386", "i686", "x86"]);
    map
});

fn get_arch_aliases(arch: &str) -> Vec<String> {
    if let Some(aliases) = ARCH_MAP.get(arch) {
        aliases.iter().map(|s| s.to_string()).collect()
    } else {
        vec![arch.to_string()]
    }
}

#[async_trait]
impl<'a> ReleaseFetcher for GithubClient<'a> {
    async fn get_releases(&self, repo: &str) -> Result<Vec<Release>> {
        let url = format!("https://api.github.com/repos/{}/releases", repo);
        let value = self.http.fetch_json(&url).await?;
        let releases: Vec<Release> =
            serde_json::from_value(value).map_err(GpmError::Serialization)?;
        Ok(releases)
    }

    async fn get_release_by_tag(&self, repo: &str, tag: &str) -> Result<Release> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            repo, tag
        );
        let value = self.http.fetch_json(&url).await?;
        let release: Release = serde_json::from_value(value).map_err(GpmError::Serialization)?;
        Ok(release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_age() {
        assert_eq!(GithubClient::parse_age("7d").unwrap(), Duration::days(7));
        assert_eq!(GithubClient::parse_age("24h").unwrap(), Duration::hours(24));
        assert_eq!(GithubClient::parse_age("1m").unwrap(), Duration::days(30));
    }

    #[test]
    fn test_select_asset_linux_x86() {
        let assets = vec![
            Asset {
                name: "rg-linux-x86_64.tar.gz".to_string(),
                browser_download_url: "".to_string(),
                size: 0,
            },
            Asset {
                name: "rg-macos-x86_64.tar.gz".to_string(),
                browser_download_url: "".to_string(),
                size: 0,
            },
            Asset {
                name: "rg-windows-x86_64.zip".to_string(),
                browser_download_url: "".to_string(),
                size: 0,
            },
        ];
        let (selected, _) = GithubClient::select_asset(&assets, "linux", "x86_64", None);
        let selected = selected.unwrap();
        assert_eq!(selected.name, "rg-linux-x86_64.tar.gz");
    }

    #[test]
    fn test_select_asset_pattern() {
        let assets = vec![
            Asset {
                name: "gpm-linux-amd64".to_string(),
                browser_download_url: "".to_string(),
                size: 0,
            },
            Asset {
                name: "gpm-helper-linux-amd64".to_string(),
                browser_download_url: "".to_string(),
                size: 0,
            },
        ];
        let (selected, _) = GithubClient::select_asset(&assets, "linux", "x86_64", Some("helper"));
        let selected = selected.unwrap();
        assert_eq!(selected.name, "gpm-helper-linux-amd64");
    }

    #[test]
    fn test_get_valid_release_skips_prerelease() {
        let releases = vec![
            Release {
                tag_name: "v2.0-beta".to_string(),
                published_at: Utc::now(),
                prerelease: true,
                draft: false,
                assets: vec![],
            },
            Release {
                tag_name: "v1.9".to_string(),
                published_at: Utc::now(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
        ];

        let result = GithubClient::get_valid_release(releases, None).unwrap().unwrap();
        assert_eq!(result.tag_name, "v1.9");
    }
}
