use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct Hunk {
    pub index: usize,
    #[serde(rename = "type")]
    pub hunk_type: String,
    pub removed: String,
    pub added: String,
}

/// Extract hunks from before/after content
pub fn get_hunks(before: &str, after: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_lines(before, after);
    let mut hunks = Vec::new();
    let mut current_removed = String::new();
    let mut current_added = String::new();
    let mut in_hunk = false;
    
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if in_hunk {
                    // End of hunk
                    let hunk_type = match (current_removed.is_empty(), current_added.is_empty()) {
                        (true, false) => "insert",
                        (false, true) => "delete",
                        _ => "replace",
                    };
                    hunks.push(Hunk {
                        index: hunks.len(),
                        hunk_type: hunk_type.to_string(),
                        removed: std::mem::take(&mut current_removed),
                        added: std::mem::take(&mut current_added),
                    });
                    in_hunk = false;
                }
            }
            ChangeTag::Delete => {
                in_hunk = true;
                current_removed.push_str(change.value());
            }
            ChangeTag::Insert => {
                in_hunk = true;
                current_added.push_str(change.value());
            }
        }
    }
    
    // Handle final hunk
    if in_hunk {
        let hunk_type = match (current_removed.is_empty(), current_added.is_empty()) {
            (true, false) => "insert",
            (false, true) => "delete",
            _ => "replace",
        };
        hunks.push(Hunk {
            index: hunks.len(),
            hunk_type: hunk_type.to_string(),
            removed: current_removed,
            added: current_added,
        });
    }
    
    hunks
}

/// Apply only selected hunks, returning the result
pub fn apply_selected_hunks(before: &str, after: &str, selected: &HashSet<usize>) -> String {
    let diff = TextDiff::from_lines(before, after);
    let mut result = String::new();
    let mut hunk_idx = 0;
    let mut in_hunk = false;
    let mut hunk_before = String::new();
    let mut hunk_after = String::new();
    
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if in_hunk {
                    // End of hunk - decide what to include
                    if selected.contains(&hunk_idx) {
                        result.push_str(&hunk_after);
                    } else {
                        result.push_str(&hunk_before);
                    }
                    hunk_before.clear();
                    hunk_after.clear();
                    hunk_idx += 1;
                    in_hunk = false;
                }
                result.push_str(change.value());
            }
            ChangeTag::Delete => {
                in_hunk = true;
                hunk_before.push_str(change.value());
            }
            ChangeTag::Insert => {
                in_hunk = true;
                hunk_after.push_str(change.value());
            }
        }
    }
    
    // Handle final hunk
    if in_hunk {
        if selected.contains(&hunk_idx) {
            result.push_str(&hunk_after);
        } else {
            result.push_str(&hunk_before);
        }
    }
    
    result
}
