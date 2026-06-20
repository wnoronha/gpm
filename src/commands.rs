use dialoguer::Confirm;
use indicatif::ProgressBar;

use crate::cli::{
    InstallArgs, LinkArgs, OutdatedArgs, PruneArgs, UninstallArgs, UnlinkArgs, UpgradeArgs,
};
use crate::errors::{GpmError, Result};
use crate::github::{GithubClient, ReleaseFetcher};
use crate::installer::Installer;
use crate::manifest::StateManager;

struct OutdatedEntry {
    name: String,
    repo: String,
    current: String,
    latest: String,
}

async fn fetch_outdated(
    github: &dyn ReleaseFetcher,
    state: &dyn StateManager,
    args: &OutdatedArgs,
    pb: Option<&ProgressBar>,
) -> Result<Vec<OutdatedEntry>> {
    let packages = state.get_packages()?;
    if packages.is_empty() {
        return Ok(Vec::new());
    }

    let mut names: Vec<_> = packages.keys().collect();
    names.sort();

    if let Some(pb) = pb {
        pb.set_length(names.len() as u64);
    }

    let mut entries = Vec::new();
    for name in names {
        if let Some(pb) = pb {
            pb.set_message(format!("Checking {name}"));
        }

        let pkg = &packages[name];
        let releases = github.get_releases(&pkg.repo).await?;
        if let Some(latest) = GithubClient::get_valid_release(releases, args.min_age.as_deref())? {
            let current = pkg.active_version.as_deref().unwrap_or("(none)");
            if current != latest.tag_name {
                entries.push(OutdatedEntry {
                    name: name.clone(),
                    repo: pkg.repo.clone(),
                    current: current.to_string(),
                    latest: latest.tag_name.clone(),
                });
            }
        }

        if let Some(pb) = pb {
            pb.inc(1);
        }
    }

    Ok(entries)
}

fn render_outdated_table(entries: &[OutdatedEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut table = crate::table::make_table(vec!["Package", "Repo", "Current", "Latest"]);
    for e in entries {
        table.add_row(vec![
            Cell::new(&e.name).fg(Color::Blue),
            Cell::new(&e.repo),
            Cell::new(&e.current).fg(Color::Yellow),
            Cell::new(&e.latest).fg(Color::Green),
        ]);
    }
    table.to_string()
}

pub async fn install(
    installer: &dyn Installer,
    github: &dyn ReleaseFetcher,
    state: &dyn StateManager,
    args: &InstallArgs,
) -> Result<()> {
    let repo = &args.repo;
    let releases = github.get_releases(repo).await?;

    let release = if let Some(tag) = &args.version {
        github.get_release_by_tag(repo, tag).await?
    } else {
        GithubClient::get_valid_release(releases, args.min_age.as_deref())?.ok_or_else(|| {
            GpmError::PackageNotFoundError(format!("No valid release found for {}", repo))
        })?
    };

    let (asset, checksum) = GithubClient::select_asset(
        &release.assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        args.pattern.as_deref(),
    );
    let asset = asset.ok_or_else(|| {
        GpmError::PackageNotFoundError(format!(
            "No suitable asset found for {} version {}",
            repo, release.tag_name
        ))
    })?;

    println!("Installing {} version {}...", repo, release.tag_name);
    let installed_files = installer
        .install_and_discover(
            repo,
            &release.tag_name,
            &asset.browser_download_url,
            &asset.name,
            checksum.as_ref().map(|a| a.browser_download_url.as_str()),
            checksum.as_ref().map(|a| a.name.as_str()),
        )
        .await?;

    let name = repo.split('/').next_back().unwrap();
    state.add_package(name, repo, &release.tag_name, &installed_files)?;
    installer.link(name, &release.tag_name, &installed_files)?;

    Ok(())
}

pub fn uninstall(
    installer: &dyn Installer,
    state: &dyn StateManager,
    args: &UninstallArgs,
) -> Result<()> {
    let pkg = state
        .get_package(&args.name)?
        .ok_or_else(|| GpmError::PackageNotFoundError(args.name.clone()))?;

    if let Some(v) = &args.pkg_version {
        let version_info = pkg.versions.get(v).ok_or_else(|| {
            GpmError::PackageNotFoundError(format!("Version {} not found for {}", v, args.name))
        })?;

        if pkg.active_version.as_deref() == Some(v) {
            installer.unlink(&args.name, &version_info.files)?;
        }

        installer.uninstall_version(&args.name, v, &version_info.files)?;
        state.remove_package(&args.name, Some(v))?;
    } else {
        if let Some(v) = &pkg.active_version {
            let version_info = pkg.versions.get(v).unwrap();
            installer.unlink(&args.name, &version_info.files)?;
        }

        for (v, info) in &pkg.versions {
            installer.uninstall_version(&args.name, v, &info.files)?;
        }
        state.remove_package(&args.name, None)?;
    }

    Ok(())
}

pub fn link(installer: &dyn Installer, state: &dyn StateManager, args: &LinkArgs) -> Result<()> {
    let pkg = state
        .get_package(&args.name)?
        .ok_or_else(|| GpmError::PackageNotFoundError(args.name.clone()))?;

    let version_info = pkg.versions.get(&args.version).ok_or_else(|| {
        GpmError::PackageNotFoundError(format!(
            "Version {} not found for package {}",
            args.version, args.name
        ))
    })?;

    if let Some(active) = &pkg.active_version {
        let active_info = pkg.versions.get(active).unwrap();
        installer.unlink(&args.name, &active_info.files)?;
    }

    installer.link(&args.name, &args.version, &version_info.files)?;
    state.set_active_version(&args.name, Some(&args.version))?;

    Ok(())
}

pub fn unlink(
    installer: &dyn Installer,
    state: &dyn StateManager,
    args: &UnlinkArgs,
) -> Result<()> {
    let pkg = state
        .get_package(&args.name)?
        .ok_or_else(|| GpmError::PackageNotFoundError(args.name.clone()))?;

    if let Some(active_info) = pkg
        .active_version
        .as_ref()
        .and_then(|v| pkg.versions.get(v))
    {
        installer.unlink(&args.name, &active_info.files)?;
        state.set_active_version(&args.name, None)?;
    }

    Ok(())
}

pub fn list(state: &dyn StateManager) -> Result<()> {
    println!("{}", format_list(state)?);
    Ok(())
}

pub async fn format_outdated(
    github: &dyn ReleaseFetcher,
    state: &dyn StateManager,
    args: &OutdatedArgs,
) -> Result<String> {
    let entries = fetch_outdated(github, state, args, None).await?;
    Ok(render_outdated_table(&entries))
}

pub async fn outdated(
    github: &dyn ReleaseFetcher,
    state: &dyn StateManager,
    args: &OutdatedArgs,
) -> Result<()> {
    let pb = crate::ui::create_count_progress_bar(0, "Checking packages…");

    let entries = fetch_outdated(github, state, args, Some(&pb)).await?;
    pb.finish_with_message("done");

    let output = render_outdated_table(&entries);
    if !output.is_empty() {
        println!("{output}");
    }
    Ok(())
}

fn resolve_targets(
    state: &dyn StateManager,
    package_name: Option<&str>,
) -> Result<std::collections::HashMap<String, crate::manifest::Package>> {
    let packages = state.get_packages()?;
    if let Some(name) = package_name {
        let pkg = packages
            .get(name)
            .ok_or_else(|| GpmError::PackageNotFoundError(name.to_string()))?;
        let mut map = std::collections::HashMap::new();
        map.insert(name.to_string(), pkg.clone());
        Ok(map)
    } else {
        Ok(packages)
    }
}

pub async fn upgrade(
    installer: &dyn Installer,
    github: &dyn ReleaseFetcher,
    state: &dyn StateManager,
    args: &UpgradeArgs,
) -> Result<()> {
    let targets = resolve_targets(state, args.package.as_deref())?;

    for (name, pkg) in targets {
        let releases = github.get_releases(&pkg.repo).await?;
        if let Some(latest) = GithubClient::get_valid_release(releases, args.min_age.as_deref())? {
            let current = pkg.active_version.as_deref().unwrap_or("(none)");
            if current != latest.tag_name {
                if !args.yes
                    && !Confirm::new()
                        .with_prompt(format!(
                            "Upgrade {} from {} to {}?",
                            name, current, latest.tag_name
                        ))
                        .interact()
                        .unwrap_or(false)
                {
                    continue;
                }

                let install_args = InstallArgs {
                    repo: pkg.repo.clone(),
                    version: Some(latest.tag_name.clone()),
                    min_age: args.min_age.clone(),
                    pattern: args.pattern.clone(),
                };
                install(installer, github, state, &install_args).await?;
            }
        }
    }

    Ok(())
}

use comfy_table::{Cell, Color};

pub fn format_list(state: &dyn StateManager) -> Result<String> {
    let packages = state.get_packages()?;
    if packages.is_empty() {
        return Ok("No packages installed.".to_string());
    }

    let mut names: Vec<_> = packages.keys().collect();
    names.sort();

    let mut table = crate::table::make_table(vec!["Package", "Repo", "Active", "Versions", "Bins"]);

    for name in names {
        let pkg = &packages[name];
        let active = pkg.active_version.as_deref().unwrap_or("(none)");
        let mut versions: Vec<_> = pkg.versions.keys().collect();
        versions.sort();

        let active_cell = if pkg.active_version.is_some() {
            Cell::new(active).fg(Color::Green)
        } else {
            Cell::new(active).fg(Color::Yellow)
        };

        let versions_str = versions
            .iter()
            .map(|v| {
                if versions.len() > 1 && pkg.active_version.as_deref() == Some(v.as_str()) {
                    format!("{}*", v)
                } else {
                    v.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let bins = pkg
            .active_version
            .as_ref()
            .and_then(|v| pkg.versions.get(v))
            .map(|info| {
                info.files
                    .iter()
                    .filter_map(|p| p.file_name().and_then(|s| s.to_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        table.add_row(vec![
            Cell::new(name).fg(Color::Blue),
            Cell::new(&pkg.repo),
            active_cell,
            Cell::new(versions_str),
            Cell::new(bins),
        ]);
    }

    Ok(table.to_string())
}

pub fn prune(installer: &dyn Installer, state: &dyn StateManager, args: &PruneArgs) -> Result<()> {
    let targets = resolve_targets(state, args.package.as_deref())?;

    for (name, pkg) in targets {
        let active = pkg.active_version.as_deref();
        let mut to_prune = Vec::new();
        for version in pkg.versions.keys() {
            if Some(version.as_str()) != active {
                to_prune.push(version.clone());
            }
        }

        if to_prune.is_empty() {
            continue;
        }

        if !args.yes
            && !Confirm::new()
                .with_prompt(format!("Prune {} versions: {}?", name, to_prune.join(", ")))
                .interact()
                .unwrap_or(false)
        {
            continue;
        }

        for version in to_prune {
            let info = pkg.versions.get(&version).unwrap();
            installer.uninstall_version(&name, &version, &info.files)?;
            state.remove_package(&name, Some(&version))?;
        }
    }

    Ok(())
}

pub async fn self_update() -> Result<()> {
    println!("Checking for gpm updates...");
    let status = tokio::task::spawn_blocking(move || {
        self_update::backends::github::Update::configure()
            .repo_owner("wnoronha")
            .repo_name("gpm")
            .bin_name("gpm")
            .show_download_progress(true)
            .current_version(env!("CARGO_PKG_VERSION"))
            .build()?
            .update()
    })
    .await
    .map_err(|e| GpmError::Unknown(e.to_string()))??;

    if status.updated() {
        println!("Successfully updated to version {}!", status.version());
    } else {
        println!("gpm is already up to date (version {}).", status.version());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use tempfile::tempdir;

    use crate::cli::OutdatedArgs;
    use crate::github::{Asset, MockReleaseFetcher, Release};
    use crate::manifest::JsonStateManager;
    use crate::manifest::StateManager;

    use crate::paths::GpmPaths;

    fn setup_state() -> (JsonStateManager, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let paths = GpmPaths::with_home(temp.path());
        (JsonStateManager::new(paths), temp)
    }

    #[test]
    fn test_format_list_empty() {
        let (state, _tmp) = setup_state();
        let output = super::format_list(&state).unwrap();
        assert_eq!(output, "No packages installed.");
    }

    #[test]
    fn test_format_list_single_package() {
        let (state, _tmp) = setup_state();
        state
            .add_package("ripgrep", "BurntSushi/ripgrep", "14.1.0", &[])
            .unwrap();

        let output = super::format_list(&state).unwrap();

        assert!(output.contains("Package"));
        assert!(output.contains("Repo"));
        assert!(output.contains("Active"));
        assert!(output.contains("Versions"));
        assert!(output.contains("Bins"));
        assert!(output.contains("ripgrep"));
        assert!(output.contains("14.1.0"));
        assert!(output.contains("BurntSushi/ripgrep"));
    }

    #[test]
    fn test_format_list_sorts_by_name() {
        let (state, _tmp) = setup_state();
        state
            .add_package("zellij", "zellij/zellij", "v0.40.0", &[])
            .unwrap();
        state
            .add_package("beads", "jcwillox/beads", "v1.0.5", &[])
            .unwrap();

        let output = super::format_list(&state).unwrap();

        let beads_pos = output.find("beads").unwrap();
        let zellij_pos = output.find("zellij").unwrap();
        assert!(beads_pos < zellij_pos, "beads should appear before zellij");
    }

    #[test]
    fn test_format_list_active_marker() {
        let (state, _tmp) = setup_state();
        state
            .add_package("ripgrep", "BurntSushi/ripgrep", "14.1.0", &[])
            .unwrap();
        state
            .add_package("ripgrep", "BurntSushi/ripgrep", "15.0.0", &[])
            .unwrap();

        let output = super::format_list(&state).unwrap();
        assert!(output.contains("15.0.0*"));
    }

    #[tokio::test]
    async fn test_format_outdated_empty_state() {
        let (state, _tmp) = setup_state();
        let mut github = MockReleaseFetcher::new();
        github.expect_get_releases().returning(|_| Ok(vec![]));
        let args = OutdatedArgs { min_age: None };
        let output = super::format_outdated(&github, &state, &args)
            .await
            .unwrap();
        assert_eq!(output, "");
    }

    #[tokio::test]
    async fn test_format_outdated_shows_outdated() {
        let (state, _tmp) = setup_state();
        state
            .add_package("ripgrep", "BurntSushi/ripgrep", "14.1.0", &[])
            .unwrap();

        let release = Release {
            tag_name: "15.0.0".to_string(),
            published_at: Utc::now(),
            prerelease: false,
            draft: false,
            assets: vec![Asset {
                name: "rg-linux.tar.gz".to_string(),
                browser_download_url: "".to_string(),
                size: 100,
            }],
        };

        let mut github = MockReleaseFetcher::new();
        github
            .expect_get_releases()
            .with(mockall::predicate::eq("BurntSushi/ripgrep"))
            .returning(move |_| Ok(vec![release.clone()]));

        let args = OutdatedArgs { min_age: None };
        let output = super::format_outdated(&github, &state, &args)
            .await
            .unwrap();

        assert!(output.contains("Package"));
        assert!(output.contains("Repo"));
        assert!(output.contains("Current"));
        assert!(output.contains("Latest"));
        assert!(output.contains("ripgrep"));
        assert!(output.contains("14.1.0"));
        assert!(output.contains("15.0.0"));
        assert!(output.contains("BurntSushi/ripgrep"));
    }

    #[tokio::test]
    async fn test_format_outdated_skips_up_to_date() {
        let (state, _tmp) = setup_state();
        state
            .add_package("ripgrep", "BurntSushi/ripgrep", "15.0.0", &[])
            .unwrap();

        let release = Release {
            tag_name: "15.0.0".to_string(),
            published_at: Utc::now(),
            prerelease: false,
            draft: false,
            assets: vec![],
        };

        let mut github = MockReleaseFetcher::new();
        github
            .expect_get_releases()
            .with(mockall::predicate::eq("BurntSushi/ripgrep"))
            .returning(move |_| Ok(vec![release.clone()]));

        let args = OutdatedArgs { min_age: None };
        let output = super::format_outdated(&github, &state, &args)
            .await
            .unwrap();

        assert_eq!(output, "");
    }
}
