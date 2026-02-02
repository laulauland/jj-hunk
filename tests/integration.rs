use std::path::{Path, PathBuf};
use std::process::Command;

/// A temporary jj repo that is fully isolated from any ambient git/jj state.
/// Cleaned up on drop.
struct TestRepo {
    dir: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join(format!("jj-hunk-test-{}-{}", name, std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();

        // Init a git-backed jj repo
        let out = Command::new("jj")
            .args(["git", "init"])
            .current_dir(&dir)
            .env("JJ_USER", "Test User")
            .env("JJ_EMAIL", "test@example.com")
            .env_remove("JJ_CONFIG")
            .output()
            .expect("jj git init failed");
        assert!(out.status.success(), "jj git init: {}", String::from_utf8_lossy(&out.stderr));

        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn write_file(&self, name: &str, content: &str) {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn jj(&self, args: &[&str]) -> std::process::Output {
        Command::new("jj")
            .args(args)
            .current_dir(&self.dir)
            .env("JJ_USER", "Test User")
            .env("JJ_EMAIL", "test@example.com")
            .env_remove("JJ_CONFIG")
            .output()
            .expect("failed to run jj")
    }

    fn jj_ok(&self, args: &[&str]) -> String {
        let out = self.jj(args);
        assert!(
            out.status.success(),
            "jj {:?} failed: stdout={} stderr={}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn hunk(&self, args: &[&str]) -> std::process::Output {
        Command::new(jj_hunk_bin())
            .args(args)
            .current_dir(&self.dir)
            .env("JJ_USER", "Test User")
            .env("JJ_EMAIL", "test@example.com")
            .env_remove("JJ_CONFIG")
            .output()
            .expect("failed to run jj-hunk")
    }

    fn hunk_ok(&self, args: &[&str]) -> String {
        let out = self.hunk(args);
        assert!(
            out.status.success(),
            "jj-hunk {:?} failed: {}{}",
            args,
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn hunk_fail(&self, args: &[&str]) -> String {
        let out = self.hunk(args);
        assert!(
            !out.status.success(),
            "jj-hunk {:?} should have failed but succeeded: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
        );
        let mut combined = String::from_utf8_lossy(&out.stderr).to_string();
        combined.push_str(&String::from_utf8_lossy(&out.stdout));
        combined
    }

    /// Get the log as a simple list of descriptions (most recent first).
    fn log_descriptions(&self) -> Vec<String> {
        let out = self.jj_ok(&[
            "log",
            "--no-graph",
            "-T",
            r#"if(description, description.first_line() ++ "\n", "")"#,
        ]);
        out.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// Show files changed in a revision.
    fn changed_files(&self, rev: &str) -> Vec<String> {
        let out = self.jj_ok(&["diff", "-r", rev, "--summary"]);
        out.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn jj_hunk_bin() -> PathBuf {
    // Use the binary built by `cargo test`
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("jj-hunk");
    assert!(path.exists(), "jj-hunk binary not found at {:?}. Run `cargo build` first.", path);
    path
}

// ---------------------------------------------------------------------------
// list -r
// ---------------------------------------------------------------------------

#[test]
fn list_rev_shows_hunks_for_non_working_copy() {
    let repo = TestRepo::new("list-rev");

    // Create a commit with some content
    repo.write_file("a.txt", "line1\nline2\n");
    repo.jj_ok(&["commit", "-m", "add a.txt"]);

    // Make a second commit that modifies a.txt
    repo.write_file("a.txt", "line1\nLINE2\n");
    repo.jj_ok(&["commit", "-m", "modify a.txt"]);

    // Working copy is now empty — list @ should have nothing
    let list_wc = repo.hunk_ok(&["list"]);
    assert!(
        !list_wc.contains("a.txt"),
        "working copy should have no hunks for a.txt"
    );

    // list -r @- should show the modification
    let list_prev = repo.hunk_ok(&["list", "-r", "@-"]);
    assert!(
        list_prev.contains("a.txt"),
        "list -r @- should show a.txt hunks:\n{}",
        list_prev
    );
    assert!(list_prev.contains("LINE2"));
}

#[test]
fn list_rev_files_mode() {
    let repo = TestRepo::new("list-rev-files");

    repo.write_file("foo.txt", "hello\n");
    repo.write_file("bar.txt", "world\n");
    repo.jj_ok(&["commit", "-m", "initial"]);

    repo.write_file("foo.txt", "hello changed\n");
    repo.write_file("bar.txt", "world changed\n");
    repo.jj_ok(&["commit", "-m", "changes"]);

    let out = repo.hunk_ok(&["list", "-r", "@-", "--files"]);
    assert!(out.contains("foo.txt"));
    assert!(out.contains("bar.txt"));
}

// ---------------------------------------------------------------------------
// split -r
// ---------------------------------------------------------------------------

#[test]
fn split_rev_splits_non_working_copy_revision() {
    let repo = TestRepo::new("split-rev");

    // Base commit
    repo.write_file("a.txt", "aaa\n");
    repo.write_file("b.txt", "bbb\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    // A commit that touches both files
    repo.write_file("a.txt", "AAA\n");
    repo.write_file("b.txt", "BBB\n");
    repo.jj_ok(&["commit", "-m", "modify both"]);

    // Now split @- keeping only a.txt changes in the first commit
    let spec = r#"{"files": {"a.txt": {"action": "keep"}}, "default": "reset"}"#;
    repo.hunk_ok(&["split", "-r", "@-", spec, "only a.txt changes"]);

    // Should now have: base -> "only a.txt changes" -> (rest) -> @
    let log = repo.log_descriptions();
    assert!(
        log.iter().any(|d| d == "only a.txt changes"),
        "should have the split commit: {:?}",
        log
    );

    // The first split commit should only touch a.txt
    // Find the commit by description
    let diff_out = repo.jj_ok(&[
        "log",
        "--no-graph",
        "-r",
        r#"description(substring:"only a.txt changes")"#,
        "-T",
        "change_id ++ \"\n\"",
    ]);
    let change_id = diff_out.trim();
    assert!(!change_id.is_empty(), "should find split commit");

    let files = repo.changed_files(change_id);
    let has_a = files.iter().any(|f| f.contains("a.txt"));
    let has_b = files.iter().any(|f| f.contains("b.txt"));
    assert!(has_a, "split commit should contain a.txt changes: {:?}", files);
    assert!(!has_b, "split commit should NOT contain b.txt changes: {:?}", files);
}

#[test]
fn split_rev_with_spec_file() {
    let repo = TestRepo::new("split-rev-specfile");

    repo.write_file("x.txt", "xxx\n");
    repo.write_file("y.txt", "yyy\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    repo.write_file("x.txt", "XXX\n");
    repo.write_file("y.txt", "YYY\n");
    repo.jj_ok(&["commit", "-m", "modify both"]);

    // Write spec to a file
    let spec_path = repo.path().join("_spec.json");
    std::fs::write(
        &spec_path,
        r#"{"files": {"x.txt": {"action": "keep"}}, "default": "reset"}"#,
    )
    .unwrap();

    repo.hunk_ok(&[
        "split",
        "-r",
        "@-",
        "-f",
        spec_path.to_str().unwrap(),
        "x only",
    ]);

    let log = repo.log_descriptions();
    assert!(log.iter().any(|d| d == "x only"), "log: {:?}", log);
}

#[test]
fn split_without_rev_operates_on_working_copy() {
    let repo = TestRepo::new("split-no-rev");

    repo.write_file("a.txt", "aaa\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    // Changes in working copy
    repo.write_file("a.txt", "AAA\n");
    repo.write_file("b.txt", "BBB\n");

    let spec = r#"{"files": {"a.txt": {"action": "keep"}}, "default": "reset"}"#;
    repo.hunk_ok(&["split", spec, "a changes"]);

    let log = repo.log_descriptions();
    assert!(
        log.iter().any(|d| d == "a changes"),
        "should have the split commit: {:?}",
        log
    );
}

// ---------------------------------------------------------------------------
// squash -r
// ---------------------------------------------------------------------------

#[test]
fn squash_rev_squashes_non_working_copy_into_parent() {
    let repo = TestRepo::new("squash-rev");

    // Base with a.txt
    repo.write_file("a.txt", "aaa\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    // Commit that adds b.txt and c.txt
    repo.write_file("b.txt", "bbb\n");
    repo.write_file("c.txt", "ccc\n");
    repo.jj_ok(&["commit", "-m", "add b and c"]);

    // Empty working copy now. Squash only b.txt from @- into its parent.
    let spec = r#"{"files": {"b.txt": {"action": "keep"}}, "default": "reset"}"#;
    repo.hunk_ok(&["squash", "-r", "@-", spec]);

    // The parent ("base") should now contain b.txt
    let base_files = repo.changed_files(r#"description(substring:"base")"#);
    let has_b = base_files.iter().any(|f| f.contains("b.txt"));
    assert!(has_b, "base should now have b.txt: {:?}", base_files);

    // @- should still have c.txt but not b.txt
    let mid_files = repo.changed_files("@-");
    let has_c = mid_files.iter().any(|f| f.contains("c.txt"));
    let still_has_b = mid_files.iter().any(|f| f.contains("b.txt"));
    assert!(has_c, "@- should still have c.txt: {:?}", mid_files);
    assert!(!still_has_b, "@- should NOT have b.txt anymore: {:?}", mid_files);
}

#[test]
fn squash_without_rev_operates_on_working_copy() {
    let repo = TestRepo::new("squash-no-rev");

    repo.write_file("a.txt", "aaa\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    // Working copy changes
    repo.write_file("a.txt", "AAA\n");
    repo.write_file("b.txt", "BBB\n");

    let spec = r#"{"files": {"a.txt": {"action": "keep"}}, "default": "reset"}"#;
    repo.hunk_ok(&["squash", spec]);

    // a.txt change should be squashed into base
    let base_files = repo.changed_files(r#"description(substring:"base")"#);
    let has_a = base_files.iter().any(|f| f.contains("a.txt"));
    assert!(has_a, "base should have a.txt: {:?}", base_files);
}

// ---------------------------------------------------------------------------
// commit (no -r, sanity check)
// ---------------------------------------------------------------------------

#[test]
fn commit_works_on_working_copy() {
    let repo = TestRepo::new("commit-wc");

    repo.write_file("a.txt", "aaa\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    repo.write_file("a.txt", "AAA\n");
    repo.write_file("b.txt", "BBB\n");

    let spec = r#"{"files": {"a.txt": {"action": "keep"}}, "default": "reset"}"#;
    repo.hunk_ok(&["commit", spec, "commit a only"]);

    let log = repo.log_descriptions();
    assert!(
        log.iter().any(|d| d == "commit a only"),
        "log: {:?}",
        log
    );

    // b.txt should still be in working copy
    let wc_files = repo.changed_files("@");
    let has_b = wc_files.iter().any(|f| f.contains("b.txt"));
    assert!(has_b, "b.txt should remain in working copy: {:?}", wc_files);
}

// ---------------------------------------------------------------------------
// error cases
// ---------------------------------------------------------------------------

#[test]
fn split_rev_invalid_revset_fails() {
    let repo = TestRepo::new("split-bad-rev");

    repo.write_file("a.txt", "aaa\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    let spec = r#"{"files": {"a.txt": {"action": "keep"}}, "default": "reset"}"#;
    let err = repo.hunk_fail(&["split", "-r", "nonexistent_bookmark", spec, "msg"]);
    assert!(
        err.contains("failed") || err.contains("error") || err.contains("Error") || err.contains("Revision"),
        "should fail with bad revset: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// list -r with spec preview
// ---------------------------------------------------------------------------

#[test]
fn list_rev_with_spec_filters_output() {
    let repo = TestRepo::new("list-rev-spec");

    repo.write_file("keep.txt", "keep\n");
    repo.write_file("drop.txt", "drop\n");
    repo.jj_ok(&["commit", "-m", "base"]);

    repo.write_file("keep.txt", "KEEP\n");
    repo.write_file("drop.txt", "DROP\n");
    repo.jj_ok(&["commit", "-m", "changes"]);

    // List with a spec that only shows keep.txt
    let spec = r#"{"files": {"keep.txt": {"action": "keep"}}, "default": "reset"}"#;
    let out = repo.hunk_ok(&["list", "-r", "@-", "--spec", spec]);
    assert!(out.contains("keep.txt"), "should show keep.txt:\n{}", out);
    assert!(!out.contains("drop.txt"), "should NOT show drop.txt:\n{}", out);
}
