# Asset Selection Logic

GPM uses a heuristic scoring system to automatically select the most appropriate asset from a GitHub Release for your current platform and architecture.

## Selection Process

The selection follows a 5-step process implemented in `src/github.rs`.

### 1. Filtering & Disqualification
Assets are immediately disqualified if they meet any of the following criteria:
- **Pattern Mismatch**: Does not contain the user-provided `--pattern` (if specified).
- **Metadata Files**: Ends with `.sha256`, `.asc`, `.sig`, `.md5`, `.txt`, or `.sha256sum`.
- **System Package Formats**: Ends with `.deb`, `.rpm`, or `.msi` (GPM prefers raw binaries or portable archives).

### 2. Platform Detection
GPM identifies your operating system and applies strict matching rules:

| Marker Group | Keywords |
| :--- | :--- |
| **Linux** | `linux`, `musl`, `tux`, `unknown-linux` |
| **macOS** | `darwin`, `macos`, `apple-darwin`, `osx` |
| **Windows**| `windows`, `pc-windows`, `win32`, `win64`, `.exe` |

**Strict Matching Rules:**
- If you are on **Linux**, any asset with a **Windows** or **macOS** marker is disqualified.
- An asset matching your OS exactly receives **+20 points**.
- A "naked" binary (no OS markers) receives **+5 points** as a fallback.

### 3. Architecture Matching
GPM maps your machine's architecture to common naming conventions:

- **x86_64**: Matches `x86_64`, `amd64`, `x64`.
- **arm64**: Matches `arm64`, `aarch64`, `armv8`.
- **i386**: Matches `i386`, `i686`, `x86`.

**Scoring:**
- Canonical or Alias Match: **+10 points**.
- No Match: **-5 points**.

### 4. Format Preference
On Unix-like systems (Linux/macOS), GPM applies a slight preference for standard archive formats:
- `.tar.gz` or `.tgz`: **+2 points**.
- `.zip`: **+1 point**.

### 5. Final Ranking
The asset with the highest cumulative score is selected for download. In the event of a tie, the first asset processed wins.

## Example Scenario
**Target System:** Linux (x86_64)

| Asset Name | Status | Points | Reason |
| :--- | :--- | :--- | :--- |
| `tool-x86_64-pc-windows-gnu.zip` | Disqualified | - | Contains `pc-windows` marker |
| `tool-i386-unknown-linux-gnu.tar.gz`| Eligible | 17 | Linux (+20), No Arch Match (-5), .tar.gz (+2) |
| `tool-x86_64-unknown-linux-musl.tar.gz`| **Winner** | **32** | Linux (+20), Arch Match (+10), .tar.gz (+2) |
| `tool-universal.sh` | Eligible | 5 | Naked binary fallback (+5) |
