use serde::Serialize;
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;
use std::fmt::Write;

pub const HUNK_ID_PREFIX: &str = "hunk-";
const CONTEXT_LINES: usize = 3;

#[derive(Debug, Clone, Serialize)]
pub struct LineRange {
    pub start: usize,
    #[serde(rename = "lines")]
    pub length: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HunkContext {
    #[serde(rename = "pre")]
    pub before: String,
    #[serde(rename = "post")]
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub index: usize,
    pub id: String,
    #[serde(rename = "type")]
    pub hunk_type: String,
    pub removed: String,
    pub added: String,
    #[serde(rename = "before")]
    pub before_range: LineRange,
    #[serde(rename = "after")]
    pub after_range: LineRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<HunkContext>,
}

#[derive(Debug, Clone, Default)]
pub struct HunkSelection {
    pub indices: HashSet<usize>,
    pub ids: HashSet<String>,
}

impl HunkSelection {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() && self.ids.is_empty()
    }

    pub fn matches(&self, index: usize, id: &str) -> bool {
        self.indices.contains(&index) || self.ids.contains(id)
    }
}

/// Extract hunks from before/after content
pub fn get_hunks(before: &str, after: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_lines(before, after);
    let before_lines = split_lines_with_endings(before);
    let mut hunks = Vec::new();
    let mut current_removed = String::new();
    let mut current_added = String::new();
    let mut in_hunk = false;
    let mut before_line = 1;
    let mut after_line = 1;
    let mut hunk_before_start = 0;
    let mut hunk_after_start = 0;
    let mut hunk_before_len = 0;
    let mut hunk_after_len = 0;

    for change in diff.iter_all_changes() {
        let line_count = count_lines(change.value());
        match change.tag() {
            ChangeTag::Equal => {
                if in_hunk {
                    finalize_hunk(
                        &mut hunks,
                        &mut current_removed,
                        &mut current_added,
                        &before_lines,
                        hunk_before_start,
                        hunk_after_start,
                        hunk_before_len,
                        hunk_after_len,
                    );
                    hunk_before_len = 0;
                    hunk_after_len = 0;
                    in_hunk = false;
                }
                before_line += line_count;
                after_line += line_count;
            }
            ChangeTag::Delete => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_before_start = before_line;
                    hunk_after_start = after_line;
                    hunk_before_len = 0;
                    hunk_after_len = 0;
                }
                current_removed.push_str(change.value());
                hunk_before_len += line_count;
                before_line += line_count;
            }
            ChangeTag::Insert => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_before_start = before_line;
                    hunk_after_start = after_line;
                    hunk_before_len = 0;
                    hunk_after_len = 0;
                }
                current_added.push_str(change.value());
                hunk_after_len += line_count;
                after_line += line_count;
            }
        }
    }

    if in_hunk {
        finalize_hunk(
            &mut hunks,
            &mut current_removed,
            &mut current_added,
            &before_lines,
            hunk_before_start,
            hunk_after_start,
            hunk_before_len,
            hunk_after_len,
        );
    }

    hunks
}

fn finalize_hunk(
    hunks: &mut Vec<Hunk>,
    current_removed: &mut String,
    current_added: &mut String,
    before_lines: &[&str],
    before_start: usize,
    after_start: usize,
    before_length: usize,
    after_length: usize,
) {
    let removed = std::mem::take(current_removed);
    let added = std::mem::take(current_added);
    let hunk_type = determine_hunk_type(&removed, &added);
    let before_range = LineRange {
        start: before_start,
        length: before_length,
    };
    let after_range = LineRange {
        start: after_start,
        length: after_length,
    };
    let context = build_context(before_lines, &before_range);
    let id = compute_hunk_id(hunk_type, &removed, &added, context.as_ref());

    hunks.push(Hunk {
        index: hunks.len(),
        id,
        hunk_type: hunk_type.to_string(),
        removed,
        added,
        before_range,
        after_range,
        context,
    });
}

/// Apply only selected hunks, returning the result
pub fn apply_selected_hunks(before: &str, after: &str, selected: &HunkSelection) -> String {
    let diff = TextDiff::from_lines(before, after);
    let before_lines = split_lines_with_endings(before);
    let mut result = String::new();
    let mut hunk_idx = 0;
    let mut in_hunk = false;
    let mut hunk_before = String::new();
    let mut hunk_after = String::new();
    let mut before_line = 1;
    let mut hunk_before_start = 0;
    let mut hunk_before_len = 0;

    for change in diff.iter_all_changes() {
        let line_count = count_lines(change.value());
        match change.tag() {
            ChangeTag::Equal => {
                if in_hunk {
                    apply_hunk_selection(
                        &mut result,
                        &mut hunk_before,
                        &mut hunk_after,
                        &before_lines,
                        selected,
                        hunk_idx,
                        hunk_before_start,
                        hunk_before_len,
                    );
                    hunk_idx += 1;
                    hunk_before_len = 0;
                    in_hunk = false;
                }
                result.push_str(change.value());
                before_line += line_count;
            }
            ChangeTag::Delete => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_before_start = before_line;
                    hunk_before_len = 0;
                }
                hunk_before.push_str(change.value());
                hunk_before_len += line_count;
                before_line += line_count;
            }
            ChangeTag::Insert => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_before_start = before_line;
                    hunk_before_len = 0;
                }
                hunk_after.push_str(change.value());
            }
        }
    }

    if in_hunk {
        apply_hunk_selection(
            &mut result,
            &mut hunk_before,
            &mut hunk_after,
            &before_lines,
            selected,
            hunk_idx,
            hunk_before_start,
            hunk_before_len,
        );
    }

    result
}

fn apply_hunk_selection(
    result: &mut String,
    hunk_before: &mut String,
    hunk_after: &mut String,
    before_lines: &[&str],
    selected: &HunkSelection,
    hunk_idx: usize,
    before_start: usize,
    before_length: usize,
) {
    let removed = std::mem::take(hunk_before);
    let added = std::mem::take(hunk_after);
    let hunk_type = determine_hunk_type(&removed, &added);
    let before_range = LineRange {
        start: before_start,
        length: before_length,
    };
    let context = build_context(before_lines, &before_range);
    let id = compute_hunk_id(hunk_type, &removed, &added, context.as_ref());

    if selected.matches(hunk_idx, &id) {
        result.push_str(&added);
    } else {
        result.push_str(&removed);
    }
}

fn determine_hunk_type(removed: &str, added: &str) -> &'static str {
    match (removed.is_empty(), added.is_empty()) {
        (true, false) => "insert",
        (false, true) => "delete",
        _ => "replace",
    }
}

fn compute_hunk_id(hunk_type: &str, removed: &str, added: &str, context: Option<&HunkContext>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"type\0");
    hasher.update(hunk_type.as_bytes());
    hasher.update(b"\0removed\0");
    hasher.update(removed.as_bytes());
    hasher.update(b"\0added\0");
    hasher.update(added.as_bytes());
    match context {
        Some(ctx) => {
            hasher.update(b"\0context\0");
            hasher.update(ctx.before.as_bytes());
            hasher.update(b"\0");
            hasher.update(ctx.after.as_bytes());
        }
        None => {
            hasher.update(b"\0context\0");
        }
    }

    let digest = hasher.finalize();
    format!("{HUNK_ID_PREFIX}{}", hex_encode(&digest))
}

pub fn normalize_hunk_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let hex = trimmed
        .strip_prefix(HUNK_ID_PREFIX)
        .or_else(|| trimmed.strip_prefix("id:"))
        .or_else(|| trimmed.strip_prefix("sha:"))
        .or_else(|| trimmed.strip_prefix("sha256:"))
        .unwrap_or(trimmed);

    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(format!("{HUNK_ID_PREFIX}{}", hex.to_lowercase()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

fn build_context(before_lines: &[&str], before_range: &LineRange) -> Option<HunkContext> {
    if before_lines.is_empty() {
        return None;
    }

    let start_idx = before_range
        .start
        .saturating_sub(1)
        .min(before_lines.len());
    let before_start = start_idx.saturating_sub(CONTEXT_LINES);
    let before_slice = before_lines.get(before_start..start_idx).unwrap_or(&[]);
    let after_start = (start_idx + before_range.length).min(before_lines.len());
    let after_end = (after_start + CONTEXT_LINES).min(before_lines.len());
    let after_slice = before_lines.get(after_start..after_end).unwrap_or(&[]);

    if before_slice.is_empty() && after_slice.is_empty() {
        return None;
    }

    Some(HunkContext {
        before: before_slice.concat(),
        after: after_slice.concat(),
    })
}

fn split_lines_with_endings(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut start = 0;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            lines.push(&text[start..=idx]);
            start = idx + 1;
        }
    }

    if start < text.len() {
        lines.push(&text[start..]);
    }

    lines
}

fn count_lines(value: &str) -> usize {
    if value.is_empty() {
        return 0;
    }

    let mut count = value.matches('\n').count();
    if !value.ends_with('\n') {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_id_is_sha256_hex_and_stable() {
        let before = "one\nTwo\nthree\n";
        let after = "one\nTWO\nthree\n";

        let hunks_first = get_hunks(before, after);
        let hunks_second = get_hunks(before, after);

        assert_eq!(hunks_first.len(), 1);
        assert_eq!(hunks_second.len(), 1);

        let id_first = &hunks_first[0].id;
        let id_second = &hunks_second[0].id;

        assert_eq!(id_first, id_second);
        assert!(id_first.starts_with(HUNK_ID_PREFIX));

        let hex = id_first.strip_prefix(HUNK_ID_PREFIX).unwrap();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hunk_id_changes_with_content() {
        let before = "alpha\nbravo\n";
        let after_one = "alpha\nbravo!\n";
        let after_two = "alpha\nbravo?\n";

        let id_one = get_hunks(before, after_one)[0].id.clone();
        let id_two = get_hunks(before, after_two)[0].id.clone();

        assert_ne!(id_one, id_two);
    }

    #[test]
    fn apply_selected_hunks_matches_by_id() {
        let before = "a\nb\nc\n";
        let after = "a\nb2\nc\n";

        let hunks = get_hunks(before, after);
        let mut selection = HunkSelection::default();
        selection.ids.insert(hunks[0].id.clone());

        let selected_result = apply_selected_hunks(before, after, &selection);
        assert_eq!(selected_result, after);

        let empty_result = apply_selected_hunks(before, after, &HunkSelection::default());
        assert_eq!(empty_result, before);
    }

    #[test]
    fn normalize_hunk_id_accepts_prefixes() {
        let before = "foo\nbar\n";
        let after = "foo\nBAR\n";
        let id = get_hunks(before, after)[0].id.clone();
        let hex = id.strip_prefix(HUNK_ID_PREFIX).unwrap();
        let expected = format!("{HUNK_ID_PREFIX}{hex}");

        assert_eq!(normalize_hunk_id(&format!("id:{hex}")).as_deref(), Some(expected.as_str()));
        assert_eq!(normalize_hunk_id(&format!("sha:{hex}")).as_deref(), Some(expected.as_str()));
        assert_eq!(normalize_hunk_id(&format!("sha256:{hex}")).as_deref(), Some(expected.as_str()));
        assert_eq!(normalize_hunk_id(hex).as_deref(), Some(expected.as_str()));
    }
}
