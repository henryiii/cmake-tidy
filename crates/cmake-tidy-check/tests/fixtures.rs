use std::fs;
use std::path::{Path, PathBuf};

use cmake_tidy_check::{CheckOptions, RuleCode, check_source};
use cmake_tidy_config::NameCase;

#[test]
fn fixture_files_do_not_trigger_parse_errors() {
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
        let result = check_source(
            &source,
            &CheckOptions {
                project_root: false,
                function_name_case: NameCase::Lower,
            },
        );

        assert!(
            result
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != RuleCode::E001),
            "fixture produced a parse error for {}: {:#?}",
            path.display(),
            result.diagnostics
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
            panic!(
                "failed to read entry under {}: {error}",
                path.display()
            )
        });
        let entry_path = entry.path();

        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            visit_directories(&entry_path, files);
        } else if entry_path.file_name().is_some_and(|name| name == "CMakeLists.txt") {
            files.push(entry_path);
        }
    }
}
