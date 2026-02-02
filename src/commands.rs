use crate::diff::{apply_selected_hunks, get_hunks, Hunk, HunkSelection};
use crate::spec::{Action, DefaultAction, FileSpec, Spec};
use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListFormat {
    Json,
    Yaml,
    Text,
}

impl Default for ListFormat {
    fn default() -> Self {
        Self::Json
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListGrouping {
    None,
    Directory,
    Extension,
    Status,
}

impl Default for ListGrouping {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BinaryMode {
    Skip,
    Mark,
    Include,
}

impl Default for BinaryMode {
    fn default() -> Self {
        Self::Mark
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMode {
    Full,
    Files,
    SpecTemplate,
}

impl Default for ListMode {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub rev: Option<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub group: ListGrouping,
    pub format: ListFormat,
    pub mode: ListMode,
    pub spec: Option<String>,
    pub spec_file: Option<String>,
    pub binary: BinaryMode,
    pub max_bytes: Option<usize>,
    pub max_lines: Option<usize>,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            rev: None,
            include: Vec::new(),
            exclude: Vec::new(),
            group: ListGrouping::default(),
            format: ListFormat::default(),
            mode: ListMode::default(),
            spec: None,
            spec_file: None,
            binary: BinaryMode::default(),
            max_bytes: None,
            max_lines: None,
        }
    }
}

impl From<Option<&str>> for ListOptions {
    fn from(rev: Option<&str>) -> Self {
        Self {
            rev: rev.map(str::to_string),
            ..Self::default()
        }
    }
}

#[derive(Debug, Serialize)]
struct ListOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<FileEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    groups: Option<Vec<ListGroup>>,
}

#[derive(Debug, Serialize)]
struct ListGroup {
    name: String,
    files: Vec<FileEntry>,
}

#[derive(Debug, Serialize)]
struct FileEntry {
    path: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rename: Option<RenameInfo>,
    hunks: Vec<Hunk>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
struct RenameInfo {
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
struct ListSummaryOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<FileSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    groups: Option<Vec<ListSummaryGroup>>,
}

#[derive(Debug, Serialize)]
struct ListSummaryGroup {
    name: String,
    files: Vec<FileSummary>,
}

#[derive(Debug, Serialize)]
struct FileSummary {
    path: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rename: Option<RenameInfo>,
    hunk_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SpecTemplateOutput {
    files: HashMap<String, SpecTemplateEntry>,
    default: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum SpecTemplateEntry {
    Ids { ids: Vec<String> },
    Action { action: String },
}

#[derive(Debug, Deserialize)]
struct DiffSummaryEntry {
    status: String,
    path: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    target: String,
}

/// List hunks in current working copy or a specific revision
pub fn list<T>(options: T) -> Result<()>
where
    T: Into<ListOptions>,
{
    let options = options.into();
    let spec = resolve_optional_spec(options.spec.as_deref(), options.spec_file.as_deref())?
        .map(|content| Spec::from_str(&content))
        .transpose()?;

    let include = normalize_patterns(&options.include);
    let exclude = normalize_patterns(&options.exclude);

    let summary_entries = read_diff_summary(options.rev.as_deref())?;
    let (before_rev, after_rev) = resolve_revisions(options.rev.as_deref());

    let mut files = Vec::new();

    for entry in summary_entries {
        let path = primary_path(&entry);
        if path.is_empty() {
            continue;
        }

        if !should_include_entry(&entry, &include, &exclude) {
            continue;
        }

        let decision = spec_decision(spec.as_ref(), &path);
        if matches!(decision, SpecDecision::Skip) {
            continue;
        }

        let file_paths = file_paths_for_entry(&entry, &path);
        let before_bytes = file_paths
            .before
            .as_deref()
            .map(|p| read_jj_file(before_rev.as_deref(), p))
            .unwrap_or_default();
        let after_bytes = file_paths
            .after
            .as_deref()
            .map(|p| read_jj_file(after_rev.as_deref(), p))
            .unwrap_or_default();

        let is_binary = is_binary_data(&before_bytes) || is_binary_data(&after_bytes);
        if is_binary && options.binary == BinaryMode::Skip {
            continue;
        }

        let should_diff = !(is_binary && options.binary == BinaryMode::Mark);
        let (before_text, before_truncated) = if should_diff {
            truncate_text(
                &String::from_utf8_lossy(&before_bytes),
                options.max_bytes,
                options.max_lines,
            )
        } else {
            (String::new(), false)
        };
        let (after_text, after_truncated) = if should_diff {
            truncate_text(
                &String::from_utf8_lossy(&after_bytes),
                options.max_bytes,
                options.max_lines,
            )
        } else {
            (String::new(), false)
        };

        let mut hunks = if should_diff {
            get_hunks(&before_text, &after_text)
        } else {
            Vec::new()
        };

        if let SpecDecision::KeepSelection(selection) = &decision {
            hunks = filter_hunks(hunks, selection);
        }

        if hunks.is_empty() && !is_binary {
            continue;
        }

        let rename = rename_info(&entry);
        let truncated = before_truncated || after_truncated;

        files.push(FileEntry {
            path,
            status: entry.status.clone(),
            rename,
            hunks,
            binary: if is_binary { Some(true) } else { None },
            truncated: if truncated { Some(true) } else { None },
        });
    }

    match options.mode {
        ListMode::Full => {
            let output = if options.group == ListGrouping::None {
                ListOutput {
                    files: Some(files),
                    groups: None,
                }
            } else {
                let groups = group_files(files, options.group);
                ListOutput {
                    files: None,
                    groups: Some(groups),
                }
            };

            match options.format {
                ListFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
                ListFormat::Yaml => {
                    println!("{}", serde_yaml::to_string(&output)?);
                }
                ListFormat::Text => {
                    print!("{}", render_text_output(&output));
                }
            }
        }
        ListMode::Files => {
            let summary = build_summary_output(files, options.group);
            match options.format {
                ListFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&summary)?);
                }
                ListFormat::Yaml => {
                    println!("{}", serde_yaml::to_string(&summary)?);
                }
                ListFormat::Text => {
                    print!("{}", render_text_summary_output(&summary));
                }
            }
        }
        ListMode::SpecTemplate => {
            if matches!(options.format, ListFormat::Text) {
                anyhow::bail!("--spec-template does not support text output (use json or yaml)");
            }
            let template = build_spec_template(files);
            match options.format {
                ListFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&template)?);
                }
                ListFormat::Yaml => {
                    println!("{}", serde_yaml::to_string(&template)?);
                }
                ListFormat::Text => {}
            }
        }
    }

    Ok(())
}

const SUMMARY_TEMPLATE: &str = r#""{\"status\":" ++ self.status().escape_json() ++ ",\"path\":" ++ self.path().display().escape_json() ++ ",\"source\":" ++ self.source().path().display().escape_json() ++ ",\"target\":" ++ self.target().path().display().escape_json() ++ "}\n""#;

struct FilePaths {
    before: Option<String>,
    after: Option<String>,
}

enum SpecDecision {
    Skip,
    KeepAll,
    KeepSelection(HunkSelection),
}

fn resolve_optional_spec(spec: Option<&str>, spec_file: Option<&str>) -> Result<Option<String>> {
    if spec.is_none() && spec_file.is_none() {
        return Ok(None);
    }

    Ok(Some(resolve_spec_input(spec, spec_file)?))
}

fn resolve_revisions(revset: Option<&str>) -> (Option<String>, Option<String>) {
    if let Some(rev) = revset {
        (Some(format!("({})^", rev)), Some(rev.to_string()))
    } else {
        (Some("@-".to_string()), None)
    }
}

fn read_diff_summary(revset: Option<&str>) -> Result<Vec<DiffSummaryEntry>> {
    let mut diff_args = vec!["diff", "--template", SUMMARY_TEMPLATE];
    if let Some(rev) = revset {
        diff_args.push("-r");
        diff_args.push(rev);
    }

    let output = Command::new("jj")
        .args(&diff_args)
        .output()
        .context("Failed to run jj diff")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("jj diff failed: {}", stderr.trim());
    }

    let summary = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for (index, line) in summary.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: DiffSummaryEntry = serde_json::from_str(line)
            .with_context(|| format!("Failed to parse diff summary line {}", index + 1))?;
        entries.push(entry);
    }

    Ok(entries)
}

fn primary_path(entry: &DiffSummaryEntry) -> String {
    if !entry.path.is_empty() {
        entry.path.clone()
    } else if !entry.target.is_empty() {
        entry.target.clone()
    } else {
        entry.source.clone()
    }
}

fn rename_info(entry: &DiffSummaryEntry) -> Option<RenameInfo> {
    match entry.status.as_str() {
        "renamed" | "copied" => {
            if entry.source.is_empty() {
                return None;
            }
            let to = if entry.target.is_empty() {
                entry.path.clone()
            } else {
                entry.target.clone()
            };
            Some(RenameInfo {
                from: entry.source.clone(),
                to,
            })
        }
        _ => None,
    }
}

fn file_paths_for_entry(entry: &DiffSummaryEntry, path: &str) -> FilePaths {
    match entry.status.as_str() {
        "added" => FilePaths {
            before: None,
            after: Some(path.to_string()),
        },
        "removed" => FilePaths {
            before: Some(path.to_string()),
            after: None,
        },
        "renamed" | "copied" => {
            let before = if entry.source.is_empty() {
                path.to_string()
            } else {
                entry.source.clone()
            };
            let after = if entry.target.is_empty() {
                path.to_string()
            } else {
                entry.target.clone()
            };
            FilePaths {
                before: Some(before),
                after: Some(after),
            }
        }
        _ => FilePaths {
            before: Some(path.to_string()),
            after: Some(path.to_string()),
        },
    }
}

fn read_jj_file(rev: Option<&str>, path: &str) -> Vec<u8> {
    let mut args = vec!["file", "show"];
    if let Some(rev) = rev {
        args.push("-r");
        args.push(rev);
    }
    args.push(path);

    Command::new("jj")
        .args(&args)
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default()
}

fn is_binary_data(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

fn truncate_text(content: &str, max_bytes: Option<usize>, max_lines: Option<usize>) -> (String, bool) {
    let mut truncated = false;
    let mut result = content.to_string();

    if let Some(max_lines) = max_lines {
        if max_lines == 0 {
            if !result.is_empty() {
                truncated = true;
            }
            result.clear();
        } else {
            let mut limited = String::new();
            let mut count = 0usize;
            for line in result.split_inclusive('\n') {
                if count >= max_lines {
                    truncated = true;
                    break;
                }
                limited.push_str(line);
                count += 1;
            }
            if truncated {
                result = limited;
            }
        }
    }

    if let Some(max_bytes) = max_bytes {
        if result.len() > max_bytes {
            let mut end = max_bytes;
            while !result.is_char_boundary(end) {
                end -= 1;
            }
            result.truncate(end);
            truncated = true;
        }
    }

    (result, truncated)
}

fn spec_decision(spec: Option<&Spec>, path: &str) -> SpecDecision {
    let Some(spec) = spec else {
        return SpecDecision::KeepAll;
    };

    if let Some(file_spec) = spec.files.get(path) {
        match file_spec {
            FileSpec::Action { action: Action::Keep } => SpecDecision::KeepAll,
            FileSpec::Action { action: Action::Reset } => SpecDecision::Skip,
            FileSpec::Selection(selection) => {
                let selection = selection.to_selection();
                if selection.is_empty() {
                    SpecDecision::Skip
                } else {
                    SpecDecision::KeepSelection(selection)
                }
            }
        }
    } else if spec.default == DefaultAction::Reset {
        SpecDecision::Skip
    } else {
        SpecDecision::KeepAll
    }
}

fn filter_hunks(hunks: Vec<Hunk>, selection: &HunkSelection) -> Vec<Hunk> {
    hunks
        .into_iter()
        .filter(|hunk| selection.matches(hunk.index, &hunk.id))
        .collect()
}

fn normalize_patterns(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .flat_map(|pattern| pattern.split(','))
        .map(|pattern| pattern.trim())
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| pattern.to_string())
        .collect()
}

fn should_include_entry(entry: &DiffSummaryEntry, include: &[String], exclude: &[String]) -> bool {
    let paths = entry_paths(entry);

    if !include.is_empty() && !paths.iter().any(|path| matches_any(include, path)) {
        return false;
    }

    if !exclude.is_empty() && paths.iter().any(|path| matches_any(exclude, path)) {
        return false;
    }

    true
}

fn entry_paths<'a>(entry: &'a DiffSummaryEntry) -> Vec<&'a str> {
    let mut paths = Vec::new();
    if !entry.path.is_empty() {
        paths.push(entry.path.as_str());
    }
    if !entry.source.is_empty() && entry.source != entry.path {
        paths.push(entry.source.as_str());
    }
    if !entry.target.is_empty() && entry.target != entry.path && entry.target != entry.source {
        paths.push(entry.target.as_str());
    }
    paths
}

fn matches_any(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|pattern| glob_match(pattern, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    let path = path.trim_start_matches("./");

    if pattern.is_empty() {
        return path.is_empty();
    }

    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match_segments(&pattern_segments, &path_segments)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }

    if pattern[0] == "**" {
        if match_segments(&pattern[1..], path) {
            return true;
        }
        if !path.is_empty() {
            return match_segments(pattern, &path[1..]);
        }
        return false;
    }

    if path.is_empty() {
        return false;
    }

    if !match_segment(pattern[0], path[0]) {
        return false;
    }

    match_segments(&pattern[1..], &path[1..])
}

fn match_segment(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; text_chars.len() + 1]; pattern_chars.len() + 1];

    dp[0][0] = true;
    for i in 1..=pattern_chars.len() {
        if pattern_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pattern_chars.len() {
        for j in 1..=text_chars.len() {
            dp[i][j] = match pattern_chars[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c == text_chars[j - 1],
            };
        }
    }

    dp[pattern_chars.len()][text_chars.len()]
}

fn group_files(files: Vec<FileEntry>, grouping: ListGrouping) -> Vec<ListGroup> {
    let mut groups: Vec<ListGroup> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for file in files {
        let key = match grouping {
            ListGrouping::Directory => directory_group(&file.path),
            ListGrouping::Extension => extension_group(&file.path),
            ListGrouping::Status => file.status.clone(),
            ListGrouping::None => String::new(),
        };

        if let Some(position) = index.get(&key).copied() {
            groups[position].files.push(file);
        } else {
            index.insert(key.clone(), groups.len());
            groups.push(ListGroup {
                name: key,
                files: vec![file],
            });
        }
    }

    groups
}

fn build_summary_output(files: Vec<FileEntry>, grouping: ListGrouping) -> ListSummaryOutput {
    let summaries: Vec<FileSummary> = files
        .into_iter()
        .map(|file| FileSummary {
            path: file.path,
            status: file.status,
            rename: file.rename,
            hunk_count: file.hunks.len(),
            binary: file.binary,
            truncated: file.truncated,
        })
        .collect();

    if grouping == ListGrouping::None {
        ListSummaryOutput {
            files: Some(summaries),
            groups: None,
        }
    } else {
        let groups = group_summaries(summaries, grouping);
        ListSummaryOutput {
            files: None,
            groups: Some(groups),
        }
    }
}

fn group_summaries(files: Vec<FileSummary>, grouping: ListGrouping) -> Vec<ListSummaryGroup> {
    let mut groups: Vec<ListSummaryGroup> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for file in files {
        let key = match grouping {
            ListGrouping::Directory => directory_group(&file.path),
            ListGrouping::Extension => extension_group(&file.path),
            ListGrouping::Status => file.status.clone(),
            ListGrouping::None => String::new(),
        };

        if let Some(position) = index.get(&key).copied() {
            groups[position].files.push(file);
        } else {
            index.insert(key.clone(), groups.len());
            groups.push(ListSummaryGroup {
                name: key,
                files: vec![file],
            });
        }
    }

    groups
}

fn build_spec_template(files: Vec<FileEntry>) -> SpecTemplateOutput {
    let mut output = HashMap::new();

    for file in files {
        if file.hunks.is_empty() {
            if file.binary == Some(true) {
                output.insert(
                    file.path,
                    SpecTemplateEntry::Action {
                        action: "keep".to_string(),
                    },
                );
            }
            continue;
        }

        let ids = file.hunks.into_iter().map(|hunk| hunk.id).collect();
        output.insert(file.path, SpecTemplateEntry::Ids { ids });
    }

    SpecTemplateOutput {
        files: output,
        default: "reset".to_string(),
    }
}

fn directory_group(path: &str) -> String {
    let path = Path::new(path);
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_string_lossy().to_string(),
        _ => ".".to_string(),
    }
}

fn extension_group(path: &str) -> String {
    let path = Path::new(path);
    match path.extension() {
        Some(ext) => ext.to_string_lossy().to_string(),
        None => "<no-ext>".to_string(),
    }
}

fn render_text_output(output: &ListOutput) -> String {
    let mut lines = Vec::new();

    if let Some(groups) = &output.groups {
        for (index, group) in groups.iter().enumerate() {
            let name = if group.name == "." || group.name.is_empty() {
                "<root>"
            } else {
                group.name.as_str()
            };
            lines.push(format!("{}:", name));
            format_files_text(&mut lines, &group.files);
            if index + 1 < groups.len() {
                lines.push(String::new());
            }
        }
    } else if let Some(files) = &output.files {
        format_files_text(&mut lines, files);
    }

    if lines.is_empty() {
        return String::new();
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn render_text_summary_output(output: &ListSummaryOutput) -> String {
    let mut lines = Vec::new();

    if let Some(groups) = &output.groups {
        for (index, group) in groups.iter().enumerate() {
            let name = if group.name == "." || group.name.is_empty() {
                "<root>"
            } else {
                group.name.as_str()
            };
            lines.push(format!("{}:", name));
            format_summary_text(&mut lines, &group.files);
            if index + 1 < groups.len() {
                lines.push(String::new());
            }
        }
    } else if let Some(files) = &output.files {
        format_summary_text(&mut lines, files);
    }

    if lines.is_empty() {
        return String::new();
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

fn format_files_text(lines: &mut Vec<String>, files: &[FileEntry]) {
    for file in files {
        lines.push(format_file_header(file));
        for hunk in &file.hunks {
            lines.push(format!(
                "  hunk {} {} {} (before {}+{} after {}+{})",
                hunk.index,
                hunk.hunk_type,
                hunk.id,
                hunk.before_range.start,
                hunk.before_range.length,
                hunk.after_range.start,
                hunk.after_range.length,
            ));
            if !hunk.removed.is_empty() {
                for line in hunk.removed.lines() {
                    lines.push(format!("    - {}", line));
                }
            }
            if !hunk.added.is_empty() {
                for line in hunk.added.lines() {
                    lines.push(format!("    + {}", line));
                }
            }
        }
    }
}

fn format_summary_text(lines: &mut Vec<String>, files: &[FileSummary]) {
    for file in files {
        let mut line = format!(
            "{} {} ({} hunks)",
            status_char(&file.status),
            file.path,
            file.hunk_count
        );
        if let Some(rename) = &file.rename {
            line.push_str(&format!(" ({} -> {})", rename.from, rename.to));
        }
        if file.binary == Some(true) {
            line.push_str(" [binary]");
        }
        if file.truncated == Some(true) {
            line.push_str(" [truncated]");
        }
        lines.push(line);
    }
}

fn format_file_header(file: &FileEntry) -> String {
    let mut header = format!("{} {}", status_char(&file.status), file.path);
    if let Some(rename) = &file.rename {
        header.push_str(&format!(" ({} -> {})", rename.from, rename.to));
    }
    if file.binary == Some(true) {
        header.push_str(" [binary]");
    }
    if file.truncated == Some(true) {
        header.push_str(" [truncated]");
    }
    header
}

fn status_char(status: &str) -> &'static str {
    match status {
        "modified" => "M",
        "added" => "A",
        "removed" => "D",
        "renamed" => "R",
        "copied" => "C",
        _ => "?",
    }
}

/// Select hunks (called by jj --tool)
pub fn select(left: &str, right: &str) -> Result<()> {
    let spec_path = std::env::var("JJ_HUNK_SELECTION").ok();
    
    let spec = if let Some(path) = spec_path {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read spec from {}", path))?;
        Spec::from_str(&content)?
    } else {
        // No selection = keep everything
        return Ok(());
    };
    
    let left_path = Path::new(left);
    let right_path = Path::new(right);
    
    // Get all files in both directories
    let left_files = list_files(left_path);
    let right_files = list_files(right_path);
    let all_files: HashSet<_> = left_files.union(&right_files).cloned().collect();
    
    for filepath in all_files {
        let file_spec = spec.files.get(&filepath);
        
        match file_spec {
            Some(FileSpec::Action { action: Action::Keep }) => {
                // Keep as-is
            }
            Some(FileSpec::Action { action: Action::Reset }) => {
                reset_file(left_path, right_path, &filepath)?;
            }
            Some(FileSpec::Selection(selection)) => {
                let selection = selection.to_selection();
                apply_hunk_selection(left_path, right_path, &filepath, &selection)?;
            }
            None => {
                // Use default
                if spec.default == DefaultAction::Reset {
                    reset_file(left_path, right_path, &filepath)?;
                }
            }
        }
    }
    
    Ok(())
}

fn list_files(dir: &Path) -> HashSet<String> {
    let mut files = HashSet::new();
    if !dir.exists() {
        return files;
    }
    
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(rel) = entry.path().strip_prefix(dir) {
                let name = rel.to_string_lossy().to_string();
                if name != "JJ-INSTRUCTIONS" {
                    files.insert(name);
                }
            }
        }
    }
    files
}

fn reset_file(left: &Path, right: &Path, filepath: &str) -> Result<()> {
    let left_file = left.join(filepath);
    let right_file = right.join(filepath);
    
    if left_file.exists() {
        if let Some(parent) = right_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&left_file, &right_file)?;
    } else if right_file.exists() {
        fs::remove_file(&right_file)?;
    }
    Ok(())
}

fn apply_hunk_selection(
    left: &Path,
    right: &Path,
    filepath: &str,
    selection: &HunkSelection,
) -> Result<()> {
    let left_file = left.join(filepath);
    let right_file = right.join(filepath);
    
    let before = if left_file.exists() {
        fs::read_to_string(&left_file)?
    } else {
        String::new()
    };
    
    let after = if right_file.exists() {
        fs::read_to_string(&right_file)?
    } else {
        return Ok(());
    };
    
    let result = apply_selected_hunks(&before, &after, selection);
    
    fs::write(&right_file, result)?;
    Ok(())
}

fn resolve_spec_input(spec: Option<&str>, spec_file: Option<&str>) -> Result<String> {
    if let Some(path) = spec_file {
        if path.is_empty() {
            anyhow::bail!("Spec file path is empty");
        }
        return fs::read_to_string(path)
            .with_context(|| format!("Failed to read spec file {}", path));
    }

    let spec = spec.ok_or_else(|| anyhow::anyhow!("Spec is required (or use --spec-file)"))?;
    if spec == "-" {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read spec from stdin")?;
        if buffer.trim().is_empty() {
            anyhow::bail!("Spec from stdin is empty");
        }
        return Ok(buffer);
    }

    Ok(spec.to_string())
}

fn run_jj_with_selection(args: &[&str], spec: Option<&str>, spec_file: Option<&str>) -> Result<()> {
    let spec_content = resolve_spec_input(spec, spec_file)?;
    let temp_file = std::env::temp_dir().join(format!("jj-hunk-{}.spec", std::process::id()));
    fs::write(&temp_file, spec_content)?;
    
    let status = Command::new("jj")
        .args(args)
        .env("JJ_HUNK_SELECTION", &temp_file)
        .status()
        .context("Failed to run jj")?;
    
    fs::remove_file(&temp_file).ok();
    
    if !status.success() {
        anyhow::bail!("jj command failed");
    }
    Ok(())
}

pub fn split(spec: Option<&str>, spec_file: Option<&str>, message: &str, rev: Option<&str>) -> Result<()> {
    let mut args = vec!["split", "--tool=jj-hunk", "-m", message];
    if let Some(rev) = rev {
        args.push("-r");
        args.push(rev);
    }
    run_jj_with_selection(&args, spec, spec_file)
}

pub fn commit(spec: Option<&str>, spec_file: Option<&str>, message: &str) -> Result<()> {
    run_jj_with_selection(
        &["commit", "-i", "--tool=jj-hunk", "-m", message],
        spec,
        spec_file,
    )
}

pub fn squash(spec: Option<&str>, spec_file: Option<&str>, rev: Option<&str>) -> Result<()> {
    let mut args = vec!["squash", "-i", "--tool=jj-hunk"];
    if let Some(rev) = rev {
        args.push("-r");
        args.push(rev);
    }
    run_jj_with_selection(&args, spec, spec_file)
}
