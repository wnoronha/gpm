// Command details and outputs for the interactive explorer
const commandsData = {
    install: {
        name: "gpm install",
        description: "Downloads, extracts, and installs a compiled binary from the specified GitHub repository.",
        options: [
            { flag: "<owner/repo>", desc: "The target GitHub repository (e.g., BurntSushi/ripgrep)." },
            { flag: "--version <tag>", desc: "Install a specific tag (skips latest release logic)." },
            { flag: "--pattern <pattern>", desc: "Filter asset filenames using a substring match." },
            { flag: "--min-age <duration>", desc: "Ignore releases newer than a given age (e.g. 7d)." }
        ],
        terminal: `$ gpm install BurntSushi/ripgrep
<span class="t-info">ℹ</span> Fetching release list for BurntSushi/ripgrep...
<span class="t-info">ℹ</span> Selected release: 14.1.0
<span class="t-info">ℹ</span> Selecting asset for linux (x86_64)...
  - ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz: 32 pts (Selected)
  - ripgrep-14.1.0-i386-unknown-linux-gnu.tar.gz: 17 pts
<span class="t-info">ℹ</span> Downloading ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz...
[========================================] 100% (5.2 MB / 5.2 MB)
<span class="t-info">ℹ</span> Extracting archive...
<span class="t-info">ℹ</span> Scanning files using magic bytes...
  - Found ELF executable: 'rg'
<span class="t-info">ℹ</span> Symlinking binary to ~/.local/bin/rg
<span class="t-success">✔</span> Installation complete: ripgrep 14.1.0 -> ~/.local/bin/rg`
    },
    uninstall: {
        name: "gpm uninstall",
        description: "Deletes installed packages and clean up symlinks.",
        options: [
            { flag: "<pkg>", desc: "The package identifier/binary name to uninstall." },
            { flag: "--pkg-version <ver>", desc: "Remove only the specified version, keeping other cached versions intact." }
        ],
        terminal: `$ gpm uninstall ripgrep --pkg-version 14.1.0
<span class="t-info">ℹ</span> Removing version 14.1.0 files from cache...
<span class="t-info">ℹ</span> Unlinking ~/.local/bin/rg
<span class="t-success">✔</span> Uninstalled ripgrep 14.1.0`
    },
    link: {
        name: "gpm link",
        description: "Manually switches the active symlink of a package to a different downloaded version in your cache.",
        options: [
            { flag: "<pkg>", desc: "The package name (e.g., ripgrep)." },
            { flag: "<version>", desc: "The downloaded version in the cache to link." }
        ],
        terminal: `$ gpm link ripgrep 13.0.0
<span class="t-info">ℹ</span> Removing active symlink ~/.local/bin/rg...
<span class="t-info">ℹ</span> Creating new symlink to version 13.0.0...
<span class="t-success">✔</span> Active version switched: ripgrep 13.0.0 -> ~/.local/bin/rg`
    },
    list: {
        name: "gpm list",
        description: "Displays all downloaded package versions in the local cache, highlighting which one is currently linked.",
        options: [],
        terminal: `$ gpm list
BurntSushi/ripgrep
  - 14.1.0 (active) -> ~/.local/bin/rg
  - 13.0.0
sharkdp/fd
  - 9.0.0 (active) -> ~/.local/bin/fd`
    },
    outdated: {
        name: "gpm outdated",
        description: "Checks GitHub for newer releases of your cached packages and displays differences.",
        options: [
            { flag: "--min-age <duration>", desc: "Filter update checks based on release publish age." }
        ],
        terminal: `$ gpm outdated
<span class="t-info">ℹ</span> Querying GitHub API for installed packages...
sharkdp/fd [9.0.0 -> 10.1.0] (new release available)
BurntSushi/ripgrep [14.1.0 -> 14.1.0] (up to date)`
    },
    upgrade: {
        name: "gpm upgrade",
        description: "Upgrades cached packages to their latest stable version.",
        options: [
            { flag: "[pkg]", desc: "Optional package name. If omitted, upgrades all outdated packages." },
            { flag: "-y", desc: "Skip confirmation prompt before running upgrades." },
            { flag: "--pattern <pattern>", desc: "Apply a name pattern filter to upgraded release assets." }
        ],
        terminal: `$ gpm upgrade -y
<span class="t-info">ℹ</span> Checking updates...
<span class="t-info">ℹ</span> Upgrading sharkdp/fd from 9.0.0 to 10.1.0...
<span class="t-info">ℹ</span> Downloading fd-v10.1.0-x86_64-unknown-linux-musl.tar.gz...
[========================================] 100% (2.1 MB / 2.1 MB)
<span class="t-info">ℹ</span> Scanning files using magic bytes...
  - Found ELF executable: 'fd'
<span class="t-info">ℹ</span> Symlinking binary to ~/.local/bin/fd
<span class="t-success">✔</span> Upgrade complete: fd 10.1.0 -> ~/.local/bin/fd`
    },
    prune: {
        name: "gpm prune",
        description: "Deletes inactive, old cached package versions to reclaim disk space, keeping only the currently active version.",
        options: [
            { flag: "[pkg]", desc: "Optional package name. If omitted, prunes all cached packages." },
            { flag: "-y", desc: "Skip confirmation prompt." }
        ],
        terminal: `$ gpm prune -y
<span class="t-info">ℹ</span> Pruning inactive package versions...
  - Removing BurntSushi/ripgrep v13.0.0
<span class="t-success">✔</span> Pruning complete. Reclaimed 14.2 MB of disk space.`
    }
};

document.addEventListener("DOMContentLoaded", () => {
    // 1. Command Explorer Tab Navigation
    const tabs = document.querySelectorAll(".cmd-tab");
    const expTitle = document.getElementById("exp-title");
    const expDesc = document.getElementById("exp-desc");
    const expOptsList = document.getElementById("exp-opts-list");
    const expOptsTitle = document.getElementById("exp-opts-title");
    const expTerminal = document.getElementById("exp-terminal");

    function loadCommand(cmdKey) {
        const cmd = commandsData[cmdKey];
        if (!cmd) return;

        // Active tab styling
        tabs.forEach(t => {
            if (t.dataset.cmd === cmdKey) {
                t.classList.add("active");
            } else {
                t.classList.remove("active");
            }
        });

        // Set text
        expTitle.textContent = cmd.name;
        expDesc.textContent = cmd.description;

        // Set options
        expOptsList.innerHTML = "";
        if (cmd.options && cmd.options.length > 0) {
            expOptsTitle.style.display = "block";
            cmd.options.forEach(opt => {
                const li = document.createElement("li");
                li.innerHTML = `<span class="opt-flag">${opt.flag}</span><span class="opt-desc">${opt.desc}</span>`;
                expOptsList.appendChild(li);
            });
        } else {
            expOptsTitle.style.display = "none";
        }

        // Set terminal output
        expTerminal.innerHTML = cmd.terminal;
    }

    // Attach click events to tabs
    tabs.forEach(tab => {
        tab.addEventListener("click", () => {
            loadCommand(tab.dataset.cmd);
        });
    });

    // Load default tab
    loadCommand("install");

    // 2. Toast notification helper
    const toast = document.createElement("div");
    toast.className = "toast";
    toast.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="feather feather-check-circle" style="color: var(--success);"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path><polyline points="22 4 12 14.01 9 11.01"></polyline></svg> Copied to clipboard`;
    document.body.appendChild(toast);

    function showToast() {
        toast.classList.add("show");
        setTimeout(() => {
            toast.classList.remove("show");
        }, 2000);
    }

    // 3. Copy-to-clipboard actions
    const copyBtns = document.querySelectorAll("[data-copy]");
    copyBtns.forEach(btn => {
        btn.addEventListener("click", (e) => {
            const textToCopy = btn.getAttribute("data-copy");
            if (textToCopy) {
                navigator.clipboard.writeText(textToCopy)
                    .then(() => {
                        showToast();
                        
                        // Micro-animation on the button itself if it contains an SVG icon or specific class
                        const icon = btn.querySelector("svg");
                        if (icon) {
                            btn.style.color = "var(--success)";
                            setTimeout(() => {
                                btn.style.color = "";
                            }, 2000);
                        }
                    })
                    .catch(err => {
                        console.error("Failed to copy: ", err);
                    });
            }
        });
    });
});
