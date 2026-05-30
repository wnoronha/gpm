# GPM - GitHub Package Manager (Rust)

A lightweight, high-performance CLI tool to install binaries directly from GitHub Releases. Built for developers who want quick tool management without the overhead of heavy package managers.

## Features
- **Single Static Binary**: No runtime dependencies.
- **Async Execution**: Powered by Tokio for fast downloads and processing.
- **Version Management**: Uses `~/.cache/gpm` for persistent storage, allowing multiple versions of the same tool to coexist.
- **Symlink Support**: Binaries are symlinked to `~/.local/bin`, making version switching instant.
- **Smart Asset Selection**: Automatically identifies the correct asset for your OS and architecture. [See details](docs/ASSET_SELECTION.md).
- **Robust Binary Discovery**: Uses Magic Byte detection (ELF, Mach-O, PE) to identify executables, even if they lack extensions or executable bits in the archive.

## Getting Started

### Installation
#### From Source
```bash
cargo install --path .
```

#### From Binary
Download the latest binary for your platform from [Releases](https://github.com/wnoronha/gpm/releases).

**macOS Note:** If you download the binary on macOS, you may need to clear the quarantine attribute for it to run:
```bash
xattr -d com.apple.quarantine gpm
```

### Quick Start
```bash
# Install a tool
gpm install BurntSushi/ripgrep

# Install a specific version
gpm install BurntSushi/ripgrep --version 14.1.0

# List what's installed
gpm list

# Link a specific version
gpm link ripgrep 14.1.0

# Unlink a tool (removes from bin, keeps in cache)
gpm unlink ripgrep

# Check for updates
gpm outdated

# Upgrade all packages
gpm upgrade -y

# Prune old versions to free up space
gpm prune -y

# Uninstall a specific version
gpm uninstall ripgrep --pkg-version 14.1.0
```

## Commands Reference

| Command | Description | Options |
| :--- | :--- | :--- |
| `install <repo>` | Install binary from `owner/repo` | `--version`: Specific version tag <br> `--min-age`: Filter by release age (e.g., `7d`) <br> `--pattern` (`-p`): Filter by asset name |
| `uninstall <pkg>` | Remove package versions | `--pkg-version`: Specific version to remove |
| `link <pkg> <ver>`| Switch active version | |
| `unlink <pkg>` | Remove symlink | |
| `list` | List installed packages | |
| `outdated` | Check for new versions | `--min-age`: Filter check by release age |
| `upgrade [pkg]` | Upgrade packages | `-y`: Auto-confirm <br> `-p`: Filter by asset name |
| `self-update` | Update `gpm` itself | |
| `prune [pkg]` | Remove inactive versions | `-y`: Auto-confirm |

## Development

### Prerequisites
- [Rust](https://rustup.rs/) (edition 2024)

### Setup & Build
```bash
cargo build
cargo test
cargo clippy
```

## Project Structure
- `src/`: Rust source code.
- `tests/`: Integration tests.
- `Cargo.toml`: Project metadata and dependencies.
- `CHANGELOG.md`: Record of all notable changes.
- `AGENTS.md`: Specialized instructions for AI-led development.
