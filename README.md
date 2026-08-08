# GPM (GitHub Package Manager)

GPM is a fast CLI tool. Use GPM to install binaries directly from GitHub releases. GPM is for developers who want to manage tools quickly without heavy package managers.

## Features

- **Single static binary**: GPM has no runtime dependencies.
- **Asynchronous operation**: GPM uses Tokio for fast downloads and file operations.
- **Version management**: GPM uses `~/.cache/gpm` for storage. You can keep multiple versions of the same tool.
- **Symlink support**: GPM creates symlinks for binaries in `~/.local/bin`. This makes version switching immediate.
- **Smart asset selection**: GPM automatically identifies the correct asset for your operating system and architecture. [Read more](docs/ASSET_SELECTION.md).
- **Reliable binary discovery**: GPM uses Magic Byte detection (ELF, Mach-O, PE) to find executables. GPM finds executables even if they do not have file extensions or executable permissions in the archive.

## How to start

### How to install

#### Compile from source code

```bash
cargo install --path .
```

#### Install from a binary

Download the latest binary for your operating system from the [Releases page](https://github.com/wnoronha/gpm/releases).

**Note for macOS:** If you download the binary on macOS, you must remove the quarantine attribute before you can run the application:

```bash
xattr -d com.apple.quarantine gpm
```

### How to configure your PATH

GPM creates symlinks for installed binaries in a standard executable directory. You must add this directory to your system `PATH`.

- **Linux**: The default directory is `~/.local/bin`.
- **macOS and Windows**: The default directory is `~/.local/bin`.

Add this command to your `~/.bashrc` or `~/.zshrc` file:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Quick start

```bash
# Install a tool
gpm install BurntSushi/ripgrep

# Install a specific version
gpm install BurntSushi/ripgrep --version 14.1.0

# Show installed packages
gpm list

# Change the active version
gpm link ripgrep 14.1.0

# Remove the symlink (removes from bin, keeps in cache)
gpm unlink ripgrep

# Check for new versions
gpm outdated

# Upgrade all packages
gpm upgrade -y

# Remove inactive versions to make free space
gpm prune -y

# Remove a specific version
gpm uninstall ripgrep --pkg-version 14.1.0
```

## Command reference

| Command | Description | Options |
| :--- | :--- | :--- |
| `install <repo>` | Install a binary from `owner/repo`. | `--version`: Select a specific version tag. <br> `--min-age`: Filter by release age (for example, `7d`). <br> `--pattern` (`-p`): Filter by asset name. |
| `uninstall <pkg>` | Remove package versions. | `--pkg-version`: Remove a specific version. |
| `link <pkg> <ver>`| Change the active version. | |
| `unlink <pkg>` | Remove the symlink. | |
| `list` | Show installed packages. | |
| `outdated` | Check for new versions. | `--min-age`: Filter the check by release age. |
| `upgrade [pkg]` | Upgrade packages. | `-y`: Confirm automatically. <br> `-p`: Filter by asset name. |
| `self-update` | Update `gpm` to the latest version. | |
| `prune [pkg]` | Remove inactive versions. | `-y`: Confirm automatically. |

## Environment variables

| Variable | Description | Default |
| :--- | :--- | :--- |
| `GPM_HOME` | The base directory for the GPM cache and configuration. | `~` |
| `GPM_BIN_DIR` | The directory where GPM creates symlinks for active binaries. | `dirs::executable_dir()` or `~/.local/bin` |

## How to develop

### Requirements

- Install [Rust](https://rustup.rs/) (edition 2024).

### How to build and test

```bash
cargo build
cargo test
cargo clippy
```

## Project structure

- `src/`: The Rust source code.
- `tests/`: The integration tests.
- `Cargo.toml`: The project metadata and dependencies.
- `CHANGELOG.md`: A record of all important changes.
- `AGENTS.md`: Special instructions for AI development.
