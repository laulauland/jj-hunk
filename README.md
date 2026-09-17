# jj-hunk

Select parts of a diff in [jj (Jujutsu)](https://github.com/martinvonz/jj), without an interactive editor.

Turn a large, multi-file diff into focused commits. Select a fix and its tests while leaving unrelated edits in the same files for another commit. Use `split`, `commit`, or `squash` with one of three selection formats: [hunkset](#1-hunkset), [JSON](#2-json), or [YAML](#3-yaml).

The hunkset query language was first created by [Yann Hodique (@sigma)](https://github.com/sigma) in his [jj-hunk fork](https://github.com/sigma/jj-hunk/tree/dev#hunkset-query-language) and proposed in [RFC: hunkset language (#7)](https://github.com/laulauland/jj-hunk/issues/7). His prototype inspired the implementation in this repository.

## One diff, two logical changes

Suppose you increased a timeout and changed logging in two existing files:

```diff
--- a/src/client.rs
+++ b/src/client.rs
-let timeout = Duration::from_secs(5);
+let timeout = Duration::from_secs(30);
 let retries = 3;
-log::debug!("request");
+log::info!("request");
```

```diff
--- a/tests/client.rs
+++ b/tests/client.rs
-assert_eq!(client.timeout().as_secs(), 5);
+assert_eq!(client.timeout().as_secs(), 30);
 assert_eq!(client.retries(), 3);
-assert_eq!(log_level(), "debug");
+assert_eq!(log_level(), "info");
```

You want one commit for the timeout fix and its test, and another for logging. Both commits will touch both files. Selecting whole files cannot separate these changes.

The examples below are alternatives: each selects only the timeout replacements, including both the removed and added lines.

## Three ways to select changes

### 1. Hunkset

Describe which edit blocks you want. Preview the selection, then split it into a commit:

```bash
jj-hunk list --format text --query 'added(content:"timeout")'
jj-hunk split --query 'added(content:"timeout")' "fix: increase request timeout"
```

The timeout replacements go into the first commit. The logging replacements remain in the other change.

You can combine content and path predicates:

```bash
jj-hunk list --query 'added(path:glob:"src/**" | glob:"tests/**", content:"timeout")'
```

A fileset selects paths; a hunkset selects change units within those paths. `changed(path:glob:"src/**")` selects all change units with a matching path. Adding a content condition narrows the result to specific blocks. You define the purpose through the query; the tool does not infer change meaning.

Use this form when a text pattern, path, change kind, or combination of predicates describes the selection. See [hunkset predicates](#hunkset-predicates) and [aliases](#aliases).

### 2. JSON

List the hunks, then select their indices or IDs explicitly:

```bash
jj-hunk list --format json
```

In the example above, the timeout replacement is hunk `0` in each file. Save this as `timeout.json`:

```json
{
  "files": {
    "src/client.rs": {"hunks": [0]},
    "tests/client.rs": {"hunks": [0]}
  },
  "default": "reset"
}
```

Preview and apply it:

```bash
jj-hunk list --spec-file timeout.json --format text
jj-hunk split --spec-file timeout.json "fix: increase request timeout"
```

Use this form when a script or agent has already chosen exact hunks. Indices start at zero within each file. You can use full IDs from list output instead; see [spec fields and IDs](#spec-fields-and-ids).

### 3. YAML

Use the same explicit selection in a format that is convenient to edit by hand:

```bash
jj-hunk list --format yaml
```

Save this as `timeout.yaml`:

```yaml
files:
  src/client.rs:
    hunks: [0]
  tests/client.rs:
    hunks: [0]
default: reset
```

Preview and apply it:

```bash
jj-hunk list --spec-file timeout.yaml --format text
jj-hunk split --spec-file timeout.yaml "fix: increase request timeout"
```

JSON and YAML are two encodings of the same selection spec, not separate query languages. In these examples, `default: reset` excludes unlisted files from the selected commit; their changes remain in the other part of the split.

For either format, you can generate an ID-based starting spec:

```bash
jj-hunk list --spec-template --format yaml
```

### Selection boundaries

All three forms select complete text blocks, not individual matching lines. An unchanged line separates the timeout and logging blocks above. If unrelated edits share one block, selecting that block includes both. Always preview before applying a selection.

Hunksets also select indivisible file changes, such as renames or binary changes. JSON/YAML hunk specs and whole-file actions have different limits; see [spec fields and IDs](#spec-fields-and-ids).

## Installation

```bash
cargo install jj-hunk
jj-hunk --help
```

Or download a prebuilt binary with [cargo-binstall](https://github.com/cargo-bins/cargo-binstall). Pass `--git` because the crates.io release is behind:

```bash
cargo binstall --git https://github.com/laulauland/jj-hunk jj-hunk
```

## Command reference

| Command | Result |
|---------|--------|
| `jj-hunk list` | Preview hunks |
| `jj-hunk split [selection] "message"` | Put the selection in the first commit; leave the rest in the other change |
| `jj-hunk commit [selection] "message"` | Commit the selection; leave the rest in the working copy |
| `jj-hunk squash [selection]` | Move the selection into the parent commit |

For `[selection]`, use either `--query 'expression'` or `--spec-file path.json` / `--spec-file path.yaml`. JSON/YAML can also be passed inline or through stdin with `-`.

```bash
# Commit matching edits without including other edits in the same files
jj-hunk commit --query 'added(content:"timeout")' "fix: increase request timeout"

# Move matching edits from a specific revision into its parent
jj-hunk squash -r @- --query 'added(content:"timeout")'
```

List, split, and squash accept `-r <rev>` (default: `@`). The revset must resolve to one revision. Commit always operates on the working copy. Revision scope stays separate from the hunkset expression.

A valid query that selects nothing is a no-op for split, commit, and squash. Query and spec inputs cannot be combined.

## Selection reference

### Hunkset predicates

The function names the operation. Arguments restrict paths or content. Multiple arguments must match the same change unit.

| Function | Selects |
|----------|---------|
| `changed(...)` | Any change unit; `content:` searches either changed-text side |
| `added(...)` | Blocks with added text, or whole added files |
| `removed(...)` | Blocks with removed text, or whole deleted files |
| `renamed(...)` | Rename units only |
| `mode_changed(...)` | Executable-mode change units only |
| `binary_changed(...)` | Binary-content change units only |
| `all()` / `none()` | The full / empty set |
| `id("hunk-<64 hex characters>")` | One exact occurrence; no abbreviated hashes |

#### Paths, files, and content

```text
added(content:"timeout")
added(path:glob:"src/**", content:"timeout")
added(file:glob:"src/**")
added(file:glob:"src/**", content:"timeout")
```

- `path:` means **where**: match either path without requiring file creation or deletion.
- `file:` means **the file itself** was added or removed. It is available only on `added()` and `removed()`, and selects whole file operations.
- `content:` searches only added text for `added()`, only removed text for `removed()`, and either changed-text side for `changed()`.

Thus `added(content:"timeout")` can select a replacement block in an existing file or an entire new file whose contents match. `added(file:glob:"src/**", content:"timeout")` selects only matching new files. An empty new file matches `file:`, but cannot match the nonempty substring `content:"timeout"`. Binary file creations and deletions can match `file:`; their bytes are not treated as searchable text.

With no arguments, `added()` and `removed()` include their respective text and whole-file operations. `changed()` is equivalent to `all()`.

An empty text file still has a content value: `added(file:glob:"**", content:exact:"")` selects empty new text files. A missing side of a text block has no value and cannot match this query.

Use `before_path:` and `after_path:` when the old/new path matters. On `renamed()`, `from:` and `to:` make the direction explicit:

```text
renamed(from:glob:"src/**", to:glob:"archive/**")

removed(
  before_path:glob:"src/**",
  after_path:glob:"archive/**",
  content:"legacy_init("
)
```

The first query selects moves, not text edits within moved files. The second selects matching text blocks within those moved files, not their rename units. Combine them with `|` to select both.

#### Pattern modifiers and composition

Content defaults to case-sensitive substring matching. Paths default to glob matching. Use an explicit modifier to choose the matching rule:

```text
added(content:regex:"timeout|deadline")
changed(path:regex:"^src/.*[.]rs$", content:substring:"authenticate")
added(file:exact:"src/client.rs")
```

Supported modifiers are `substring:`, `exact:`, `glob:`, and `regex:`. Glob patterns support `*`, `**`, and `?`; this is not the full jj fileset language. Regex patterns use Rust's `regex` syntax, with explicit inline flags such as `(?i)`. Escape backslashes inside query strings, for example `added(content:regex:"timeout\\s*=\\s*\\d+")`.

Patterns within a field can use `|`, `&`, binary `~`, unary `~`, and parentheses. Each pattern expression is evaluated against one available path or text side at a time. Missing sides do not match, even for negated patterns. Top-level set operators combine whole change units:

```text
removed(path:glob:"src/**", content:"fetch_user(")
& added(content:"load_user(")
```

This selects blocks that remove the old call and add the new call. Both conditions must match the same block. A multiline content match can span consecutive lines on one side, but cannot cross from removed text to added text. Unchanged context is never searched.

Unknown fields, repeated fields, unsupported fields for an operation, and invalid patterns are errors. Alias parameters remain set expressions, not pattern parameters. See the [0.5.1 migration notes](docs/releases/v0.5.1.md) for changes from the old function spellings.

### Aliases

Use repeatable global `--alias 'name(parameters)=expression'` options. Parameters are set expressions and are referenced with `parameter()` in the alias body. For example:

```bash
jj-hunk \
  --alias 'timeout_change()=added(content:"timeout")' \
  --alias 'handwritten(selection)=selection() ~ changed(path:glob:"generated/**")' \
  list --query 'handwritten(timeout_change())'
```

The same options work with `split`, `commit`, and `squash`. Alias arguments are parsed expressions, so substitution preserves parentheses and operator precedence. Alias definition syntax, names, builtin collisions, duplicate names and parameters, and body syntax are validated when the environment is built. Wrong arity, unknown names, cycles, more than 32 nested expansions, and more than 10,000 expanded expression or pattern-expression nodes are checked when a query references the alias. All checks for the requested query finish before a mutation starts; unused alias bodies are not recursively resolved.

Aliases can also be stored in effective `jj` configuration. Quote each signature because TOML bare keys cannot contain parentheses:

```toml
[hunkset-aliases]
"timeout_change()" = 'added(content:"timeout")'
"handwritten(selection)" = 'selection() ~ changed(path:glob:"generated/**")'
```

`jj-hunk` reads the effective values through `jj config`, so normal user, repository, and workspace precedence applies. A `--alias` definition replaces a configured definition with the same alias name, including its parameter signature. Duplicate configured names and duplicate command-line names remain errors because alias overloading is not supported.

The `hunkset` library does not read CLI or repository configuration. `jj-hunk` loads configuration and supplies an explicit environment. Other callers construct aliases with `AliasDefinition::new`, collect them with `AliasEnvironment::new`, and call `evaluate_with_aliases`. `evaluate` uses an empty alias environment.

### Spec fields and IDs

Specs can be **JSON or YAML**. Inline JSON is convenient for short specs; use `--spec-file` or stdin for larger ones. You can select text hunks by index (`hunks`) or by exact `ids` emitted by `jj-hunk list`. IDs use the form `hunk-<64 lowercase hexadecimal characters>`. They bind an occurrence to the complete materialized before/after comparison, paths, file bytes, executable state, and text location. The same comparison produces the same IDs, independent of display limits. Any comparison change can regenerate all IDs, so do not reuse a saved ID after a revision or working-copy change. Releases before this contract used content/context hashes; regenerate saved specs because there is no legacy-ID fallback. Occurrence IDs currently require a Unix platform so executable state is part of the comparison. `hunks` entries may also be ID strings.

```json
{
  "files": {
    "path/to/file": {"hunks": [0, "hunk-7c3d...", 2]},
    "path/to/other": {"ids": ["hunk-9a2b..."]},
    "path/to/another": {"action": "keep"},
    "path/to/skip": {"action": "reset"}
  },
  "default": "reset"
}
```

- `{"hunks": [indices|ids]}` — select by index (0-based) or id string
- `{"ids": ["hunk-..."]}` — select hunks by id from `jj-hunk list`
- `{"action": "keep"}` — include all changes in the file in the selection
- `{"action": "reset"}` — exclude the file's changes from the selection
- `"default"` — action for unlisted files (`"keep"` or `"reset"`)

`ids` and `hunks` are merged if both are provided. Use `jj-hunk list --spec-template` to generate an id-based starting spec.

Query preview returns changed text blocks in `hunks` and selected indivisible changes in `file_units`. Both forms include exact occurrence IDs for `id()` queries. Creation and deletion units include their complete added or removed text, including an empty string for empty files. Rename, mode, and binary units are separate from text blocks in the same file. The legacy spec schema keeps text-hunk IDs and whole-file keep/reset actions. Its single-hunk projection for a nonempty creation or deletion shares the corresponding file-unit ID. Legacy hunk or ID selection rejects renamed files; use a query with `id()` for those occurrences. Rename, mode, binary, and empty-file unit IDs are query-only. Empty-file and other file-only occurrences are available with `list --query 'all()'`. Copies, conflicts, symlinks, trees, and submodules report explicit unsupported-input errors. `--query` cannot be combined with `--spec`, `--spec-file`, `--include`, `--exclude`, `--files`, or `--spec-template`. `--max-bytes` and `--max-lines` limit displayed selected text after the query evaluates complete content.

### List options

- `--rev <revset>` — diff the revision against its parent (revset must resolve to a single revision)
- `--format json|yaml|text` — output format (default: json)
- `--include <glob>` / `--exclude <glob>` — filter paths (repeatable, supports `**`, `*`, `?`)
- `--group none|directory|extension|status` — group output
- `--binary skip|mark|include` — binary handling (default: mark)
- `--max-bytes <n>` / `--max-lines <n>` — limit displayed changed text after IDs and queries use the complete diff
- `--spec <json|yaml>` / `--spec-file <path>` — preview using a spec filter
- `--query <expression>` — select occurrences with the hunkset predicates above
- `--files` — list files with hunk counts only
- `--spec-template` — emit a spec template (JSON/YAML only)

### List output

IDs are abbreviated here for readability. Use full IDs from actual list output in a selection.

```json
{
  "files": [
    {
      "path": "src/lib.rs",
      "status": "modified",
      "hunks": [
        {
          "id": "hunk-4c1b1b3...",
          "index": 0,
          "type": "replace",
          "removed": "old_fn()\n",
          "added": "new_fn()\n",
          "before": {"start": 10, "length": 1},
          "after": {"start": 10, "length": 1},
          "context": {"pre": "// prev\n", "post": "// next\n"}
        }
      ]
    },
    {
      "path": "src/main.rs",
      "status": "deleted",
      "hunks": [
        {
          "id": "hunk-771ad9f...",
          "index": 0,
          "type": "delete",
          "removed": "dead_code()\n",
          "added": "",
          "before": {"start": 1, "length": 1},
          "after": {"start": 1, "length": 0}
        }
      ]
    }
  ]
}
```

- `files` is a list of file entries. Each entry includes `status`, optional `rename`, and `hunks`.
- Each hunk includes a comparison-bound `id` (SHA-256), `index`, line ranges (`before`/`after`), and optional `context`.
- When grouped (`--group`), output uses `groups: [{name, files}]` instead of `files`.

### List Modes

```bash
# Files-only summary
jj-hunk list --files --format text

# Spec template (ids, default reset)
jj-hunk list --spec-template --format yaml
```

### Filtering and Grouping

```bash
jj-hunk list --include 'src/**' --exclude '**/*.test.rs' --group directory
```

## How it works

jj-hunk uses jj's diff-tool interface. It evaluates your hunkset or JSON/YAML selection against a materialized comparison, prepares the selected changes, and lets jj create or rewrite the commits. Commands supply the tool configuration automatically; no interactive editor is required.

## License

MIT
