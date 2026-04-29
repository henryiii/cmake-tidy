use std::fs;
use std::path::{Path, PathBuf};

use cmake_tidy_lexer::tokenize;
use cmake_tidy_parser::parse_file;

#[test]
fn tokenizes_and_parses_all_fixture_cmake_files() {
    let fixtures_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/files")
        .canonicalize()
        .expect("fixtures directory should exist");

    let fixture_paths = collect_cmake_files(&fixtures_root);
    assert!(
        !fixture_paths.is_empty(),
        "expected at least one CMakeLists.txt fixture under {}",
        fixtures_root.display()
    );

    for path in fixture_paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));

        let tokens = tokenize(&source);
        assert!(
            !tokens.is_empty(),
            "tokenizer returned no tokens for {}",
            path.display()
        );

        let parsed = parse_file(&source);
        assert!(
            parsed.errors.is_empty(),
            "parser reported errors for {}: {:#?}",
            path.display(),
            parsed.errors
        );
        assert!(
            !parsed.syntax.items.is_empty(),
            "parser produced an empty file AST for {}",
            path.display()
        );
    }
}

fn collect_cmake_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    visit_directories(root, &mut files);
    files.sort();
    files
}

fn visit_directories(path: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", path.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("failed to read entry under {}: {error}", path.display())
        });
        let entry_path = entry.path();

        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            visit_directories(&entry_path, files);
        } else if entry_path
            .file_name()
            .is_some_and(|name| name == "CMakeLists.txt")
        {
            files.push(entry_path);
        }
    }
}
