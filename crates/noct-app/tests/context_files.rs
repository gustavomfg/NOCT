//! The path rules of the project tree and of the files read for a call: no link followed, ignored folders skipped,
//! sensitive files blocked and never read.

mod common;

use std::fs;

use noct_app::context_files::{read_project_text, walk};

#[test]
fn the_tree_skips_ignored_folders_and_links_and_blocks_sensitive_files() {
    let folder = common::Scratch::new("context-tree");
    let root = folder.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules/x")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn a() {}\n").unwrap();
    fs::write(root.join("node_modules/x/i.js"), "x").unwrap();
    fs::write(root.join(".env"), "KEY=1").unwrap();
    fs::write(root.join("README.md"), "# r\n").unwrap();
    #[cfg(unix)]
    {
        let outside = common::Scratch::new("context-outside");
        fs::write(outside.path().join("secret.txt"), "fora").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), root.join("src/fora.txt"))
            .unwrap();
        assert_eq!(read_project_text(root, "src/fora.txt"), None);
        assert_eq!(read_project_text(root, "link/secret.txt"), None);
    }
    let (entries, truncated) = walk(root).expect("the tree is read");
    assert!(!truncated);
    let paths: Vec<(&str, bool)> = entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.blocked))
        .collect();
    assert_eq!(
        paths,
        [(".env", true), ("README.md", false), ("src/lib.rs", false)]
    );
    assert_eq!(read_project_text(root, ".env"), None);
    assert_eq!(read_project_text(root, "../x"), None);
    assert_eq!(
        read_project_text(root, "src/lib.rs").as_deref(),
        Some("fn a() {}\n")
    );
}
