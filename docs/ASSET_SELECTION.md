# Asset selection logic

GPM uses a scoring system. This system automatically selects the best asset from a GitHub release for your operating system and architecture.

## Selection process

The selection uses a 5-step process in `src/github.rs`.

### 1. Filter and remove

GPM removes assets if they match these conditions:

- **Pattern mismatch**: The asset name does not contain the `--pattern` string.
- **Metadata files**: The file name ends with `.sha256`, `.asc`, `.sig`, `.md5`, `.txt`, or `.sha256sum`.
- **System package formats**: The file name ends with `.deb`, `.rpm`, or `.msi`. GPM prefers raw binaries or portable archives.

### 2. Find the operating system

GPM identifies your operating system and applies these matching rules:

| Marker group | Keywords |
| :--- | :--- |
| **Linux** | `linux`, `musl`, `tux`, `unknown-linux` |
| **macOS** | `darwin`, `macos`, `apple-darwin`, `osx` |
| **Windows**| `windows`, `pc-windows`, `win32`, `win64`, `.exe` |

**Strict matching rules:**

- If you use **Linux**, GPM removes any asset with a **Windows** or **macOS** marker.
- GPM adds **20 points** to an asset that matches your operating system.
- GPM adds **5 points** to an asset that has no operating system markers.

### 3. Find the architecture

GPM compares your computer architecture to common names:

- **x86_64**: Matches `x86_64`, `amd64`, `x64`.
- **arm64**: Matches `arm64`, `aarch64`, `armv8`.
- **i386**: Matches `i386`, `i686`, `x86`.

**Scores:**

- Match: **+10 points**.
- No match: **-5 points**.

### 4. Format preference

On Linux and macOS, GPM prefers standard archive formats:

- `.tar.gz` or `.tgz`: **+2 points**.
- `.zip`: **+1 point**.

### 5. Final scores

GPM selects the asset with the highest score. If two assets have the same score, GPM selects the first asset.

## Example

**Target system:** Linux (x86_64)

| Asset name | Status | Points | Reason |
| :--- | :--- | :--- | :--- |
| `tool-x86_64-pc-windows-gnu.zip` | Removed | - | The name has a `pc-windows` marker. |
| `tool-i386-unknown-linux-gnu.tar.gz`| Kept | 17 | Linux (+20), no architecture match (-5), .tar.gz (+2). |
| `tool-x86_64-unknown-linux-musl.tar.gz`| **Winner** | **32** | Linux (+20), architecture match (+10), .tar.gz (+2). |
| `tool-universal.sh` | Kept | 5 | No markers (+5). |
