use tempfile::tempdir;

#[test]
fn install_extracts_bundled_docs_with_stamp() {
    let root = tempdir().unwrap();

    let docs_dir = cassady::docs::install(root.path()).unwrap();

    assert_eq!(docs_dir, root.path().join("docs"));
    assert!(docs_dir.join("README.md").is_file());
    assert_eq!(
        std::fs::read_to_string(docs_dir.join(".cass-docs-hash"))
            .unwrap()
            .trim(),
        cassady::docs::docs_hash()
    );

    let second_install = cassady::docs::install(root.path()).unwrap();
    assert_eq!(second_install, docs_dir);
}
