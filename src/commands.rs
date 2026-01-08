use crate::diff::{apply_selected_hunks, get_hunks};
use crate::spec::{Action, DefaultAction, FileSpec, Spec};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

/// List hunks in current working copy
pub fn list() -> Result<()> {
    // Get changed files from jj
    let output = Command::new("jj")
        .args(["diff", "--summary"])
        .output()
        .context("Failed to run jj diff")?;
    
    let summary = String::from_utf8_lossy(&output.stdout);
    let mut result: HashMap<String, Vec<crate::diff::Hunk>> = HashMap::new();
    
    for line in summary.lines() {
        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }
        let filepath = parts[1].trim();
        
        // Get before content
        let before = Command::new("jj")
            .args(["file", "show", "-r", "@-", filepath])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        
        // Get after content
        let after = Command::new("jj")
            .args(["file", "show", filepath])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        
        let hunks = get_hunks(&before, &after);
        if !hunks.is_empty() {
            result.insert(filepath.to_string(), hunks);
        }
    }
    
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Select hunks (called by jj --tool)
pub fn select(left: &str, right: &str) -> Result<()> {
    let spec_path = std::env::var("JJ_HUNK_SELECTION").ok();
    
    let spec = if let Some(path) = spec_path {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read spec from {}", path))?;
        Spec::from_json(&content)?
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
            Some(FileSpec::Hunks { hunks }) => {
                apply_hunk_selection(left_path, right_path, &filepath, hunks)?;
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

fn apply_hunk_selection(left: &Path, right: &Path, filepath: &str, hunks: &[usize]) -> Result<()> {
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
    
    let selected: HashSet<usize> = hunks.iter().copied().collect();
    let result = apply_selected_hunks(&before, &after, &selected);
    
    fs::write(&right_file, result)?;
    Ok(())
}

fn run_jj_with_selection(args: &[&str], spec: &str) -> Result<()> {
    let temp_file = std::env::temp_dir().join(format!("jj-hunk-{}.json", std::process::id()));
    fs::write(&temp_file, spec)?;
    
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

pub fn split(spec: &str, message: &str) -> Result<()> {
    run_jj_with_selection(&["split", "--tool=jj-hunk", "-m", message], spec)
}

pub fn commit(spec: &str, message: &str) -> Result<()> {
    run_jj_with_selection(&["commit", "-i", "--tool=jj-hunk", "-m", message], spec)
}

pub fn squash(spec: &str) -> Result<()> {
    run_jj_with_selection(&["squash", "-i", "--tool=jj-hunk"], spec)
}
