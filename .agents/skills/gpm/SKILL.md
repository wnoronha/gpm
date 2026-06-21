---
name: gpm
description: Manage and install command-line tools and binary dependencies using the GitHub Package Manager (GPM).
---

# GPM (GitHub Package Manager) Skill

Use the `gpm` command-line tool to manage binary CLI dependencies (such as compilers, test runners, or utility tools) inside your workspace.

## Agent Guidelines

When a task requires a CLI tool that is missing from the environment (e.g., `ripgrep`, `fd`, `git-delta`):
1. **Check Cache**: Run `gpm list` to see if a version is already downloaded locally.
2. **Install**: Use `gpm install <owner/repo>` to download, extract, and symlink the correct executable for your host platform and architecture.
3. **Switch Version**: If a specific version is required, use `gpm link <pkg> <version>` to toggle the active symlink.
4. **Maintenance**: To free up disk space, run `gpm prune -y` to clean up old, inactive cached versions.

## Commands Reference

```bash
gpm install BurntSushi/ripgrep          # Install latest version
gpm install BurntSushi/ripgrep --version 14.1.0  # Install specific version
gpm list                               # List cached versions
gpm link ripgrep 14.1.0                # Switch active symlink target
gpm outdated                           # Check for updates on GitHub
gpm upgrade -y                         # Upgrade all cached packages
gpm prune -y                           # Delete inactive versions
```
