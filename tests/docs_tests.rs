use std::path::Path;
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

#[test]
fn bundled_docs_links_resolve() {
    let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");

    for entry in std::fs::read_dir(&docs_root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for target in markdown_links(&text) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
            {
                continue;
            }
            let target_path = target.split('#').next().unwrap_or(target.as_str());
            if target_path.is_empty() {
                continue;
            }
            assert!(
                docs_root.join(target_path).is_file(),
                "{} links to missing bundled doc: {target}",
                path.display()
            );
        }
    }
}

#[test]
fn expected_bundled_docs_exist() {
    let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    for file in [
        "README.md",
        "commands.md",
        "configuration.md",
        "providers.md",
        "access-modes.md",
        "workflows.md",
        "troubleshooting.md",
        "platforms.md",
        "glossary.md",
    ] {
        assert!(docs_root.join(file).is_file(), "missing docs/{file}");
    }
}

fn markdown_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close) = text[i..].find("](") {
                let start = i + close + 2;
                if let Some(end) = text[start..].find(')') {
                    links.push(text[start..start + end].to_string());
                    i = start + end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    links
}
