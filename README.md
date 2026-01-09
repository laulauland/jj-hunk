# jj-hunk

Programmatic hunk selection for [jj (Jujutsu)](https://github.com/martinvonz/jj).

Select specific diff hunks when splitting, committing, or squashing—without interactive UI. Designed for AI agents and automation.

## Installation

### 1. Install the binary

```bash
cargo install jj-hunk
```

### 2. Configure jj

Add to `~/.jjconfig.toml`:

```toml
[merge-tools.jj-hunk]
program = "jj-hunk"
edit-args = ["select", "$left", "$right"]
```

### 3. Verify

```bash
jj-hunk --help
```

## Quick Start

```bash
# See what hunks exist in your changes
jj-hunk list

# Split changes: hunks 0,1 of foo.rs → first commit, rest → second
jj-hunk split '{"files": {"src/foo.rs": {"hunks": [0, 1]}}, "default": "reset"}' "first commit"

# Commit specific files, leave rest in working copy
jj-hunk commit '{"files": {"src/fix.rs": {"action": "keep"}}, "default": "reset"}' "bug fix"

# Squash specific changes into parent
jj-hunk squash '{"files": {"src/cleanup.rs": {"action": "keep"}}, "default": "reset"}'
```

## Commands

| Command | Description |
|---------|-------------|
| `jj-hunk list` | List all hunks as JSON |
| `jj-hunk split <spec> <message>` | Split changes into two commits |
| `jj-hunk commit <spec> <message>` | Commit selected hunks |
| `jj-hunk squash <spec>` | Squash selected hunks into parent |

## Spec Format

```json
{
  "files": {
    "path/to/file": {"hunks": [0, 2]},
    "path/to/other": {"action": "keep"},
    "path/to/another": {"action": "reset"}
  },
  "default": "reset"
}
```

- `{"hunks": [indices]}` — select specific hunks by index
- `{"action": "keep"}` — keep all changes in file
- `{"action": "reset"}` — discard all changes in file
- `"default"` — action for unlisted files (`"keep"` or `"reset"`)

## Example Output

```bash
$ jj-hunk list
{
  "src/lib.rs": [
    {"index": 0, "type": "replace", "removed": "old_fn()\n", "added": "new_fn()\n"},
    {"index": 1, "type": "insert", "removed": "", "added": "// new comment\n"}
  ],
  "src/main.rs": [
    {"index": 0, "type": "delete", "removed": "dead_code()\n", "added": ""}
  ]
}
```

## How It Works

jj-hunk integrates with jj's `--tool` mechanism:

1. You run `jj-hunk split/commit/squash` with a JSON spec
2. jj-hunk writes the spec to a temp file and sets `JJ_HUNK_SELECTION` env var
3. jj invokes `jj-hunk select $left $right` as the diff tool
4. jj-hunk reads the spec and modifies `$right` to include only selected hunks
5. jj snapshots the result

For direct control:

```bash
echo '{"files": {"src/foo.rs": {"hunks": [0]}}}' > /tmp/spec.json
JJ_HUNK_SELECTION=/tmp/spec.json jj split -i --tool=jj-hunk -m "message"
```

## Use Cases

### AI Agents

The primary use case. AI agents can create clean, logical commits without interactive prompts. Instead of dumping all changes into one commit, an agent can:

1. Analyze changes with `jj-hunk list`
2. Group files by logical concern (schema, services, tests, etc.)
3. Split iteratively to create a narrative commit history

The JSON spec format is easy for LLMs to construct programmatically.

### Clean History Workflow

Reorganize messy development history into reviewer-friendly commits. Squash everything, then split by concern:

```bash
jj squash --from 'all:trunk()..@-' --into @
jj edit @
jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}}, "default": "reset"}' "feat: add schema"
jj-hunk split '{"files": {"src/api/routes.ts": {"action": "keep"}}, "default": "reset"}' "feat: add routes"
jj describe -m "feat: add UI"
```

See `.claude/commands/clean-history.md` for a complete workflow.

### CI/CD Automation

Script commit splitting in pipelines. Enforce commit hygiene rules, auto-split by file patterns, or validate that commits are properly scoped.

### Partial Commits

Keep experimental code in working copy while committing only the finished parts:

```bash
jj-hunk commit '{"files": {"src/fix.rs": {"action": "keep"}}, "default": "reset"}' "fix: handle edge case"
# Experimental changes remain uncommitted
```

## Claude Code Integration

This repo includes a Claude Code command for the clean history workflow:

```
/clean-history [bookmark-name]
```

The command guides through squashing, splitting, and creating a PR with narrative-quality commits.

## License

MIT
