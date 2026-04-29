#![allow(clippy::literal_string_with_formatting_args)]

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures_root = manifest_dir.join("../../tests/files");

    let mut files = Vec::new();
    if fixtures_root.exists() {
        collect_cmake_files(&fixtures_root, &mut files);
    }
    files.sort();

    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR should be set");
    let out_file = Path::new(&out_dir).join("fixture_tests.rs");
    let mut out = fs::File::create(&out_file).expect("should be able to create output file");

    writeln!(
        out,
        "use std::fs;\nuse std::path::Path;\nuse cmake_tidy_lexer::tokenize;\nuse cmake_tidy_parser::parse_file;\n"
    )
    .unwrap();

    for file in &files {
        let relative = file
            .strip_prefix(&fixtures_root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let test_name = format_test_name(&relative);

        writeln!(out, "#[test]").unwrap();
        writeln!(out, "fn {test_name}() {{").unwrap();
        writeln!(
            out,
            "    let manifest_dir = Path::new(env!(\"CARGO_MANIFEST_DIR\"));"
        )
        .unwrap();
        writeln!(
            out,
            "    let path = manifest_dir.join(\"../../tests/files/{relative}\");"
        )
        .unwrap();
        writeln!(out, "    let source = fs::read_to_string(&path)").unwrap();
        writeln!(
            out,
            "        .unwrap_or_else(|error| panic!(\"failed to read {{}}: {{error}}\", path.display()));"
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "    let tokens = tokenize(&source);").unwrap();
        writeln!(
            out,
            "    assert!(!tokens.is_empty(), \"tokenizer returned no tokens for {{}}\", path.display());"
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "    let parsed = parse_file(&source);").unwrap();
        writeln!(
            out,
            "    assert!(parsed.errors.is_empty(), \"parser reported errors for {{}}: {{:#?}}\", path.display(), parsed.errors);"
        )
        .unwrap();
        writeln!(
            out,
            "    assert!(!parsed.syntax.items.is_empty(), \"parser produced an empty file AST for {{}}\", path.display());"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();

        println!("cargo:rerun-if-changed=../../tests/files/{relative}");
    }

    println!("cargo:rerun-if-changed=../../tests/files");
}

fn collect_cmake_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_cmake_files(&path, out);
            } else if path.file_name().is_some_and(|n| n == "CMakeLists.txt") {
                out.push(path);
            }
        }
    }
}

fn format_test_name(rel_path: &str) -> String {
    rel_path.replace(['/', '\\', '.', '-'], "_").to_lowercase()
}
