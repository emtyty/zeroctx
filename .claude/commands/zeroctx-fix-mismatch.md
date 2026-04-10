---
name: zeroctx-fix-mismatch
description: "Read ZeroCTX mismatch log, fix error patterns in source, rebuild and deploy binary"
---

# /zeroctx-fix-mismatch

Read the ZeroCTX mismatch log, analyze failing patterns, fix source code, rebuild, and deploy.

## Usage
```
/zeroctx-fix-mismatch
/zeroctx-fix-mismatch --category autofix
/zeroctx-fix-mismatch --category compression
/zeroctx-fix-mismatch --days 7
```

## Behavioral Flow

### 1. Read mismatch log
```bash
zero mismatch                              # all categories
zero mismatch --category autofix          # fix failures only
zero mismatch --category output_filter
zero mismatch --category compression
zero mismatch --category intent_routing
```

### 2. Analyze each unresolved event
Fields to check:
- `detected` — which pattern matched / which fix command ran
- `actual` — what actually happened (fix failed, error persists, over-compressed)
- `input_snippet` — the original stderr/stdout that triggered it
- `context` — language, command, file path

### 3. Map category to source file
| Category | File |
|---|---|
| `autofix` | `src/errors/patterns.rs` |
| `output_filter` | `src/filters/` |
| `intent_routing` | `src/agents/router.rs` |
| `compression` | `src/compression/mod.rs` |

Paths are relative to the ZeroCTX project root. Find it with:
```bash
ZERO_BIN=$(which zero)
# Project root is typically where zero was built from
git -C "$(dirname $ZERO_BIN)" rev-parse --show-toplevel 2>/dev/null || echo "locate manually"
```

### 4. Fix the pattern
- Read the relevant file with `cat` (avoid Read tool — ZeroCTX hook compresses it)
- Update regex, fix command, or explanation
- Verify no existing patterns are broken

### 5. Build the binary

**Detect project root first:**
```bash
PROJECT_ROOT=$(git rev-parse --show-toplevel)   # if inside the repo
cd "$PROJECT_ROOT"
```

**macOS / Linux:**
```bash
cargo build --release
```

**Windows (Git Bash) — requires MSVC + Windows SDK:**
```bash
# Auto-detect installed MSVC version
MSVC_BASE=$(ls -d "/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/"* 2>/dev/null | tail -1)
SDK_VER=$(ls "/c/Program Files (x86)/Windows Kits/10/Lib/" 2>/dev/null | sort -V | tail -1)
SDK_BASE="C:/Program Files (x86)/Windows Kits/10"

export LIB="$MSVC_BASE/lib/x64;$SDK_BASE/Lib/$SDK_VER/um/x64;$SDK_BASE/Lib/$SDK_VER/ucrt/x64"
export INCLUDE="$MSVC_BASE/include;$SDK_BASE/Include/$SDK_VER/ucrt;$SDK_BASE/Include/$SDK_VER/um;$SDK_BASE/Include/$SDK_VER/shared"
export PATH="$MSVC_BASE/bin/Hostx64/x64:$HOME/.cargo/bin:$PATH"

cargo build --release
```

> If build fails with linker errors, MSVC Build Tools may not be installed.
> Install with: `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`

### 6. Deploy binary

**macOS / Linux:**
```bash
cp target/release/zero ~/.local/bin/zero
chmod +x ~/.local/bin/zero
zero version
```

**Windows:**
```bash
cp target/release/zero.exe ~/.local/bin/zero.exe
zero version
```

### 7. Mark events resolved
```bash
zero feedback <EVENT_ID> "fixed: <description of change>"
```

---

## Examples

### Autofix: fix command insufficient
```
detected: pattern=rust_unresolved_import, fix_cmd=cargo add serde
actual:   fix failed — needs feature flag
```
Fix in `src/errors/patterns.rs` → `rust_patterns()`:
```rust
command: Some(format!("cargo add {} --features derive", package)),
```

### Autofix: missing pattern
```
detected: (no pattern matched)
actual:   error[E0277]: the trait bound `X: Y` is not satisfied
```
Add new `ErrorPattern` to `rust_patterns()` in `src/errors/patterns.rs`.

### Compression: tree-sitter too aggressive
```
detected: tree_sitter, lang=Rust, savings=97%
actual:   1042→15 lines (loses fn body content)
```
Fix in `src/compression/mod.rs` — lower the savings threshold or add fallback to `basic_compress`.

### Output filter: over-filtering
```
detected: applied git_diff filter
actual:   important error lines removed
```
Fix in `src/filters/git.rs` — adjust `keep_lines_matching` regex.

---

## Notes
- Always use `cat` or `Bash` to read source files — the ZeroCTX Read hook compresses them
- Mismatch DB (Windows): `%LOCALAPPDATA%\zeroctx\mismatch.db`
- Mismatch DB (macOS/Linux): `~/.local/share/zeroctx/mismatch.db`
- Tracking DB (Windows): `%APPDATA%\zeroctx\tracking.db`
- Tracking DB (macOS/Linux): `~/.local/share/zeroctx/tracking.db`
