//! v0.23.0 — Codex mode file trust: automatic backed-up writes under the
//! user's home folder (Full Access profile scope, never system scope), with
//! the same protected-path guards. Integration test (own process) so the
//! HOME/USERPROFILE redirection cannot race with other tests.

use std::path::Path;

fn temp_home(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("atria-codex-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn codex_profile_scope_writes_auto_with_backup_and_blocks_protected() {
    let home = temp_home("profile");
    let data_root = home.join(".atria-state");
    std::fs::create_dir_all(&data_root).unwrap();
    // Redirect the profile root into the temp home for this process.
    std::env::set_var("USERPROFILE", &home);
    std::env::set_var("HOME", &home);

    // 1) Protected path is blocked even in profile scope (.ssh key material).
    let blocked = atria_core::tools::auto_file_write_full_access(
        &serde_json::json!({ "path": ".ssh/id_ed25519", "content": "not-allowed" }),
        Path::new("/nonexistent-workspace"),
        true,  // profile scope (codex tier)
        false, // never system scope
        &data_root,
    );
    assert!(blocked.is_error, "protected path must be blocked: {}", blocked.output);

    // 2) Absolute paths stay rejected in profile scope (relative only).
    let abs = std::env::temp_dir().join("atria-codex-abs.txt");
    let rejected = atria_core::tools::auto_file_write_full_access(
        &serde_json::json!({ "path": abs.to_string_lossy(), "content": "x" }),
        Path::new("/nonexistent-workspace"),
        true,
        false,
        &data_root,
    );
    assert!(rejected.is_error, "absolute path must be rejected: {}", rejected.output);

    // 3) A pre-existing project file is updated automatically; a restorable
    //    backup of the old content is kept.
    let target_dir = home.join("projects").join("demo");
    std::fs::create_dir_all(&target_dir).unwrap();
    let target = target_dir.join("main.rs");
    std::fs::write(&target, "fn old() {}\n").unwrap();
    let ok = atria_core::tools::auto_file_write_full_access(
        &serde_json::json!({ "path": "projects/demo/main.rs", "content": "fn new() {}\n" }),
        Path::new("/nonexistent-workspace"),
        true,
        false,
        &data_root,
    );
    assert!(!ok.is_error, "codex profile write failed: {}", ok.output);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "fn new() {}\n");
    assert!(ok.output.contains("خودکار"), "auto-apply note missing: {}", ok.output);
    let backup_id = ok
        .output
        .split_once("[ATRIA_BACKUP:")
        .expect("backup marker")
        .1
        .split(']')
        .next()
        .expect("id")
        .to_string();
    let bak = data_root.join("backups").join(format!("{backup_id}.bak"));
    assert!(bak.exists(), "backup file missing: {bak:?}");
    assert_eq!(std::fs::read_to_string(&bak).unwrap(), "fn old() {}\n");
    // The undo entry is resolvable through the public restore path: the file
    // on disk must be back to the pre-edit content.
    let restored = atria_core::tools::restore_file_backup(&backup_id, &data_root)
        .expect("restore must work");
    assert!(restored.contains("Restored"), "restore message: {restored}");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "fn old() {}\n");

    let _ = std::fs::remove_dir_all(&home);
}
