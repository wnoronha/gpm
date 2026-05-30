use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gpm")]
#[command(version)]
#[command(about = "GPM: A lightweight version manager for GitHub binaries.", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package from GitHub
    Install(InstallArgs),
    /// Uninstall an installed package
    Uninstall(UninstallArgs),
    /// Link a specific version of a package
    Link(LinkArgs),
    /// Unlink a package (remove from bin dir)
    Unlink(UnlinkArgs),
    /// List all installed packages
    List,
    /// Check for outdated packages
    Outdated(OutdatedArgs),
    /// Upgrade installed packages
    Upgrade(UpgradeArgs),
    /// Update gpm to the latest version
    SelfUpdate,
    /// Remove inactive package versions
    Prune(PruneArgs),
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// GitHub repository in 'owner/repo' format
    pub repo: String,
    /// Specific version tag to install
    #[arg(long)]
    pub version: Option<String>,
    /// Minimum age of the release (e.g. 7d, 24h, 1m)
    #[arg(long)]
    pub min_age: Option<String>,
    /// Pattern to match in the asset name (useful for multi-binary repos)
    #[arg(short, long)]
    pub pattern: Option<String>,
}

#[derive(Args, Debug)]
pub struct UninstallArgs {
    /// Name of the package to uninstall
    pub name: String,
    /// Specific version to uninstall
    #[arg(long)]
    pub pkg_version: Option<String>,
}

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// Name of the package
    pub name: String,
    /// Specific version to link
    pub version: String,
}

#[derive(Args, Debug)]
pub struct UnlinkArgs {
    /// Name of the package to unlink
    pub name: String,
}

#[derive(Args, Debug)]
pub struct OutdatedArgs {
    /// Minimum age of the release (e.g. 7d, 24h, 1m)
    #[arg(long)]
    pub min_age: Option<String>,
}

#[derive(Args, Debug)]
pub struct UpgradeArgs {
    /// Name of the package to upgrade (optional)
    pub package: Option<String>,
    /// Minimum age of the release (e.g. 7d, 24h, 1m)
    #[arg(long)]
    pub min_age: Option<String>,
    /// Pattern to match in the asset name (useful for multi-binary repos)
    #[arg(short, long)]
    pub pattern: Option<String>,
    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct PruneArgs {
    /// Name of the package to prune versions from (optional)
    pub package: Option<String>,
    /// Skip confirmation prompt
    #[arg(short, long)]
    pub yes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_parsing() {
        let cli = Cli::parse_from([
            "gpm",
            "install",
            "owner/repo",
            "--version",
            "1.0.0",
            "-p",
            "bin",
        ]);
        if let Commands::Install(args) = cli.command {
            assert_eq!(args.repo, "owner/repo");
            assert_eq!(args.version, Some("1.0.0".to_string()));
            assert_eq!(args.pattern, Some("bin".to_string()));
        } else {
            panic!("Incorrect command parsed");
        }
    }

    #[test]
    fn test_upgrade_parsing() {
        let cli = Cli::parse_from(["gpm", "upgrade", "ripgrep", "-y"]);
        if let Commands::Upgrade(args) = cli.command {
            assert_eq!(args.package, Some("ripgrep".to_string()));
            assert!(args.yes);
        } else {
            panic!("Incorrect command parsed");
        }
    }

    #[test]
    fn test_list_parsing() {
        let cli = Cli::parse_from(["gpm", "list"]);
        assert!(matches!(cli.command, Commands::List));
    }
}
