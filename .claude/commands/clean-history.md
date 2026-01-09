# Inspired by git-based "reimplement branch with clean history" workflow
# Key difference: jj doesn't require reimplementing - just reorganize existing changes

---
description: Reorganize the current branch into a clean, narrative-quality commit history
argument-hint: [bookmark-name]
allowed-tools: Bash(jj:*), Bash(jj-hunk:*), Bash(gh:*)
model: opus
---

## Context

- Current change: !`jj log -r @ --no-graph`
- Bookmarks: !`jj bookmark list`
- Commits since main: !`jj log -r 'trunk()..@' --no-graph`
- Full diff against main: !`jj diff -r 'trunk()' --stat`

## Task

Reorganize changes into a clean, narrative-quality commit history suitable for reviewer comprehension.

**Bookmark Name**: Use `$ARGUMENTS` if provided.

### Steps

1. **Validate the working copy**
   - Ensure no conflicts: `jj status`
   - Review the full scope of changes to understand what needs organizing

2. **Combine all changes into one commit**
   ```bash
   # Squash all commits since trunk into the current commit
   jj squash --from 'all:trunk()..@-' --into @
   ```

3. **Edit the combined commit**
   ```bash
   jj edit @
   ```

4. **Plan the commit storyline**
   - Study the changes: `jj-hunk list | jq 'keys'`
   - Group files by logical concern (e.g., schema, migrations, services, tests)
   - Order commits as a narrative: setup → core logic → integration → polish

5. **Split iteratively using jj-hunk**

   First, inspect the hunks to understand what you're working with:

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

   **File-level selection** — when all hunks in a file belong together:

   ```bash
   # Keep entire file, reset everything else
   jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}}, "default": "reset"}' "feat: add database schema"
   ```

   **Hunk-level selection** — when a file has mixed concerns:

   ```bash
   # src/lib/utils.ts has refactoring (hunks 0, 2) and new feature (hunk 1)
   # Extract just the refactoring hunks
   jj-hunk split '{"files": {"src/lib/utils.ts": {"hunks": [0, 2]}}, "default": "reset"}' "refactor: clean up utils"

   # Now hunk 1 remains in working copy for the feature commit
   ```

   **Mixed selection** — combine file-level and hunk-level:

   ```bash
   # Keep all of schema.ts, but only hunk 0 from routes.ts
   jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}, "src/api/routes.ts": {"hunks": [0]}}, "default": "reset"}' "feat: add users endpoint"

   # Next commit: remaining routes.ts hunk 1
   jj-hunk split '{"files": {"src/api/routes.ts": {"hunks": [1]}}, "default": "reset"}' "feat: add posts endpoint"
   ```

   **Typical narrative sequence:**

   ```bash
   # 1. Infrastructure/setup first
   jj-hunk split '{"files": {"src/db/schema.ts": {"action": "keep"}, "drizzle.config.ts": {"action": "keep"}}, "default": "reset"}' "feat: add database schema"

   # 2. Core logic
   jj-hunk split '{"files": {"src/lib/utils.ts": {"hunks": [0, 2]}}, "default": "reset"}' "refactor: prepare utils for new feature"

   # 3. Feature implementation
   jj-hunk split '{"files": {"src/lib/utils.ts": {"action": "keep"}, "src/api/routes.ts": {"hunks": [0]}}, "default": "reset"}' "feat: add user routes"

   # 4. Remaining changes described as final commit
   jj describe -m "feat: add post routes"
   ```

6. **Describe the final commit**
   ```bash
   jj describe -m "feat: final piece of the implementation"
   ```

7. **Verify the result**
   ```bash
   # Check the commit structure
   jj log -r 'trunk()..@'

   # Verify each commit has sensible content
   jj diff -r <rev> --stat

   # Confirm total diff matches original intent
   jj diff -r 'trunk()' --stat
   ```

8. **Set bookmark and push**
   ```bash
   jj bookmark set <name> -r @
   jj git push --bookmark <name>
   ```

9. **Create pull request**
   ```bash
   gh pr create --title "feat: description" --body "$(cat <<'EOF'
   ## Summary
   - bullet points

   ## Test plan
   - [ ] verification steps
   EOF
   )"
   ```

### Spec Reference

| Spec | Effect |
|------|--------|
| `{"action": "keep"}` | Include all changes in file |
| `{"action": "reset"}` | Exclude file from this commit |
| `{"hunks": [0, 2]}` | Include only hunks 0 and 2 |
| `"default": "reset"` | Unlisted files excluded (safer) |
| `"default": "keep"` | Unlisted files included |

### Rules

- Never add yourself as an author or contributor
- Never include "Generated with Claude Code" or "Co-Authored-By" lines
- Each commit should introduce a single coherent idea
- Commit messages should read like a tutorial progression
- The final diff against trunk must match the original intent
