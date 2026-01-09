---
name: jj-hunk
description: Programmatic hunk selection for jj (Jujutsu). Use when splitting commits, making partial commits, or selectively squashing changes without interactive UI.
---

# jj-hunk: Programmatic Hunk Selection

Use `jj-hunk` for non-interactive hunk selection in jj. Essential for AI agents that need to create clean, logical commits from mixed changes.

## When to Use This Skill

- Splitting a commit into multiple logical commits
- Committing only specific hunks (partial commit)
- Squashing only certain changes into parent
- Any hunk selection that would normally require `jj split -i` or `jj squash -i`

## Setup

```bash
cargo install jj-hunk
```

Add to `~/.jjconfig.toml`:
```toml
[merge-tools.jj-hunk]
program = "jj-hunk"
edit-args = ["select", "$left", "$right"]
```

## Core Workflow

### 1. List Hunks

```bash
jj-hunk list
```

Output (JSON):
```json
{
  "src/foo.rs": [
    {"index": 0, "type": "replace", "removed": "old\n", "added": "new\n"},
    {"index": 1, "type": "insert", "removed": "", "added": "// added\n"}
  ],
  "src/bar.rs": [
    {"index": 0, "type": "delete", "removed": "removed\n", "added": ""}
  ]
}
```

### 2. Build a Spec

Select hunks by index or use file-level actions:

```json
{
  "files": {
    "src/foo.rs": {"hunks": [0]},
    "src/bar.rs": {"action": "keep"},
    "src/baz.rs": {"action": "reset"}
  },
  "default": "reset"
}
```

| Spec | Effect |
|------|--------|
| `{"hunks": [0, 2]}` | Include only hunks 0 and 2 |
| `{"action": "keep"}` | Include all changes |
| `{"action": "reset"}` | Discard all changes |
| `"default": "reset"` | Unlisted files are discarded |
| `"default": "keep"` | Unlisted files are kept |

### 3. Execute

```bash
# Split: selected hunks → first commit, rest → second commit
jj-hunk split '<spec>' "commit message"

# Commit: selected hunks committed, rest stays in working copy
jj-hunk commit '<spec>' "commit message"

# Squash: selected hunks squashed into parent
jj-hunk squash '<spec>'
```

## Examples

### Split Mixed Changes into Logical Commits

You have refactoring and a new feature mixed together:

```bash
# 1. See what hunks exist
jj-hunk list

# 2. Split out the refactoring first
jj-hunk split '{"files": {"src/lib.rs": {"hunks": [0, 1]}}, "default": "reset"}' \
  "refactor: extract helper function"

# 3. Remaining changes become second commit
jj describe -m "feat: add new feature"
```

### Commit Only Part of Your Changes

Keep experimental code in working copy while committing the fix:

```bash
jj-hunk commit '{"files": {"src/bug.rs": {"action": "keep"}}, "default": "reset"}' \
  "fix: handle null case"
```

### Squash Specific Files into Parent

```bash
jj-hunk squash '{"files": {"src/tests.rs": {"action": "keep"}}, "default": "reset"}'
```

### Keep Everything Except One File

```bash
jj-hunk split '{"files": {"src/wip.rs": {"action": "reset"}}, "default": "keep"}' \
  "feat: complete implementation"
```

## Direct jj --tool Usage

The commands above are wrappers. For direct control:

```bash
# Write spec to file
echo '{"files": {"src/foo.rs": {"hunks": [0]}}, "default": "reset"}' > /tmp/spec.json

# Run jj with the tool
JJ_HUNK_SELECTION=/tmp/spec.json jj split -i --tool=jj-hunk -m "message"
```

## Hunk Types

| Type | Meaning |
|------|---------|
| `insert` | New lines added |
| `delete` | Lines removed |
| `replace` | Lines changed (removed + added) |

## Agent Workflow Examples

### Understanding the Output

Always start by inspecting what hunks exist:

```bash
jj-hunk list
```

Example output:
```json
{
  "src/db/schema.ts": [
    {"index": 0, "type": "insert", "added": "import { pgTable }...\n"},
    {"index": 1, "type": "insert", "added": "export const users = pgTable...\n"},
    {"index": 2, "type": "insert", "added": "export const posts = pgTable...\n"}
  ],
  "src/api/routes.ts": [
    {"index": 0, "type": "replace", "removed": "// TODO\n", "added": "app.get('/users', ...);\n"},
    {"index": 1, "type": "insert", "added": "app.get('/posts', ...);\n"}
  ],
  "src/lib/utils.ts": [
    {"index": 0, "type": "replace", "removed": "function old()...\n", "added": "function new()...\n"},
    {"index": 1, "type": "insert", "added": "export function helper()...\n"},
    {"index": 2, "type": "delete", "removed": "// dead code\n"}
  ]
}
```

### File-Level Selection

When all hunks in a file belong to the same logical change:

```bash
# Keep entire file, reset everything else
jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}}, "default": "reset"}' "feat: add database schema"
```

### Hunk-Level Selection

When a single file has mixed concerns (most powerful feature):

```bash
# src/lib/utils.ts has:
#   - hunks 0, 2: refactoring (rename + delete dead code)
#   - hunk 1: new feature (helper function)

# Extract just the refactoring
jj-hunk split '{"files": {"src/lib/utils.ts": {"hunks": [0, 2]}}, "default": "reset"}' "refactor: clean up utils"

# Hunk 1 remains in working copy for the next commit
jj describe -m "feat: add helper function"
```

### Mixed Selection

Combine file-level and hunk-level in one spec:

```bash
# Keep all of schema.ts + only hunk 0 from routes.ts
jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}, "src/api/routes.ts": {"hunks": [0]}}, "default": "reset"}' "feat: add users table and endpoint"

# Next: remaining routes.ts hunk
jj-hunk split '{"files": {"src/api/routes.ts": {"action": "keep"}}, "default": "reset"}' "feat: add posts endpoint"

# Final: utils changes
jj describe -m "refactor: utils cleanup"
```

### Complete Workflow Example

Starting with a messy commit containing schema, API, and refactoring changes:

```bash
# 1. Edit the commit
jj edit <revision>

# 2. Inspect all hunks
jj-hunk list

# 3. Split in narrative order

# Infrastructure first
jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}}, "default": "reset"}' "feat: add database schema"

# Refactoring second (specific hunks from utils.ts)
jj-hunk split '{"files": {"src/lib/utils.ts": {"hunks": [0, 2]}}, "default": "reset"}' "refactor: clean up utils"

# Feature using the refactored code
jj-hunk split '{"files": {"src/lib/utils.ts": {"action": "keep"}, "src/api/routes.ts": {"hunks": [0]}}, "default": "reset"}' "feat: add users endpoint"

# Remaining changes
jj describe -m "feat: add posts endpoint"

# 4. Verify
jj log -r 'trunk()..@'
```

### Verifying Splits

After splitting, verify each commit has the right content:

```bash
# Check stats for each commit
jj diff -r <rev1> --stat
jj diff -r <rev2> --stat

# Or view the log
jj log
```

## Tips

- **Always list first**: Run `jj-hunk list` to see hunk indices before building specs
- **Use default wisely**: `"default": "reset"` is safer (explicit inclusion), `"default": "keep"` is convenient for excluding specific files
- **Combine with jj**: After splitting, use `jj describe` to refine commit messages
- **Exact paths required**: File paths must match exactly (e.g., `"src/lib.rs"` not `"src/"`)
