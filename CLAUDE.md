# ZeroCTX — Token-Efficient AI Coding Tool

ZeroCTX (`zero`) is installed on this system. Use it to save tokens and time.

## Commands You Should Use

### Run dev commands through ZeroCTX (filtered output)

Instead of running dev commands directly, route them through `zero rewrite-exec --` to get compressed output:

```bash
# Instead of:    cargo test
# Use:           zero rewrite-exec -- cargo test

# Instead of:    python -m pytest
# Use:           zero rewrite-exec -- python -m pytest

# Instead of:    npm run build
# Use:           zero rewrite-exec -- npm run build

# Instead of:    dotnet test
# Use:           zero rewrite-exec -- dotnet test

# Instead of:    git diff HEAD~5
# Use:           zero rewrite-exec -- git diff HEAD~5
```

This filters output (removes noise, keeps errors/failures), tracks token savings, and auto-fixes common errors (like missing packages).

**When to use**: Any `git`, `cargo`, `pytest`, `npm`, `dotnet`, `eslint`, `tsc`, `ruff`, `mypy` command.
**When NOT to use**: Interactive commands (`ssh`, `vim`), commands with redirects (`> file`), or when you need raw unfiltered output.

### Convert markdown to HTML

```bash
zero convert path/to/file.md --format html -o output.html
```

Use this when the user asks to convert/export any markdown file to HTML or PDF.

### Check token savings

```bash
zero stats
zero stats --daily
```

Use this when the user asks about token savings, efficiency, or how ZeroCTX is performing.

### Export reports

```bash
zero export --format html -o report.html    # Styled HTML dashboard
zero export --format json                    # JSON to stdout
zero export --format csv -o data.csv         # CSV table
```

Use this when the user asks for a report or export of tracking data.

### View configuration

```bash
zero config
```

### Check mismatch/quality report

```bash
zero mismatch                                  # Full report (last 30 days)
zero mismatch --days 7                         # Last 7 days
zero mismatch --category autofix --limit 10    # Drill into auto-fix issues
zero mismatch --category intent_routing        # Drill into routing issues
zero mismatch --category output_filter         # Drill into filter issues
zero mismatch --category compression           # Drill into compression issues
zero mismatch --json                           # Export as JSON
```

Use this when the user asks about quality, accuracy, mismatches, or wants to improve ZeroCTX's behavior.

### Submit feedback on a mismatch

```bash
zero feedback <EVENT_ID> "description of what went wrong"
```

Use this when the user reports that ZeroCTX made a wrong decision (wrong intent, over-filtered output, failed auto-fix, etc.).

## Error Auto-Fix

ZeroCTX has 62 error patterns that auto-fix common errors WITHOUT needing your reasoning. When you run a command through `zero rewrite-exec --` and it fails:

- `ModuleNotFoundError: No module named 'X'` → auto-runs `pip install X`
- `Cannot find module 'X'` → auto-runs `npm install X`
- `CS0246: type not found` → auto-runs `dotnet add package X`
- `E0433: unresolved import` → auto-runs `cargo add X`

If the auto-fix succeeds, just report it to the user and rerun the original command.

## Important Notes

- `zero` is at: `zero.exe` (on PATH)
- `jq` is required for hooks (also on PATH)
- Token savings are tracked in SQLite at `~/.zeroctx/tracking.db` (Windows: `%APPDATA%\zeroctx\tracking.db`)
- Mismatch/quality data is tracked at `~/.local/share/zeroctx/mismatch.db`
- Configuration: `.zeroctx/config.toml` (project) or `~/.zeroctx/config.toml` (global)
