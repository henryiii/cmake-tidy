use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[test]
fn check_reports_diagnostics_for_invalid_root_file() -> Result<()> {
    let temp_dir = create_root_file("project()\nproject(example)\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("W203"));
    assert!(stdout.contains("W202"));
    assert!(stdout.contains("W301"));
    assert!(!stdout.contains("W302"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_succeeds_for_valid_root_file() -> Result<()> {
    let temp_dir = create_root_file("cmake_minimum_required(VERSION 3.30)\nproject(example)\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_select_filters_diagnostics() -> Result<()> {
    let temp_dir = create_root_file("project()\nproject(example)\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg("--select")
        .arg("W203")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("W203"));
    assert!(!stdout.contains("W202"));
    assert!(!stdout.contains("W301"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_ignore_filters_diagnostics() -> Result<()> {
    let temp_dir = create_root_file("project()\nproject(example)\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg("--ignore")
        .arg("W203,W301")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("W202"));
    assert!(!stdout.contains("W203"));
    assert!(!stdout.contains("W301"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_respects_file_level_noqa() -> Result<()> {
    let temp_dir = create_root_file("# noqa\nproject()\nproject(example)\n")?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_accepts_direct_cmake_module_input() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    let module = temp_dir.join("tooling.cmake");
    fs::write(&module, "add_library(example STATIC main.cpp)\n")
        .with_context(|| format!("failed to write {}", module.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&module)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_select_all_enables_naming_diagnostics_for_modules() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    let module = temp_dir.join("tooling.cmake");
    fs::write(&module, "ADD_LIBRARY(example STATIC main.cpp)\n")
        .with_context(|| format!("failed to write {}", module.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg("--select")
        .arg("ALL")
        .arg(&module)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("N001"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_can_fix_naming_from_cli() -> Result<()> {
    let temp_dir = create_root_file("ADD_LIBRARY(example STATIC main.cpp)\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg("--select")
        .arg("N")
        .arg("--fix")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        "add_library(example STATIC main.cpp)\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_can_fix_naming_from_config() -> Result<()> {
    let temp_dir = create_root_file("add_library(example STATIC main.cpp)\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");
    fs::write(
        temp_dir.join("cmake-tidy.toml"),
        "fix = true\n[lint]\nselect = [\"N\"]\nfunction-name-case = \"upper\"\n",
    )
    .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .current_dir(&temp_dir)
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        "ADD_LIBRARY(example STATIC main.cpp)\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_respects_per_file_ignores_from_config() -> Result<()> {
    let temp_dir = create_root_file("project()\nproject(example)\n")?;
    fs::write(
        temp_dir.join("cmake-tidy.toml"),
        "[lint]\nselect = [\"W\"]\n\n[lint.per-file-ignores]\n\"CMakeLists.txt\" = [\"W203\", \"W301\"]\n",
    )
    .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .current_dir(&temp_dir)
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("W202"));
    assert!(!stdout.contains("W203"));
    assert!(!stdout.contains("W301"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_errors_when_no_cmake_files_are_found() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    fs::write(temp_dir.join("notes.txt"), "hello\n")
        .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).context("stderr should be valid UTF-8")?;
    assert!(stderr.contains("no CMake files found"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_reports_invalid_configuration() -> Result<()> {
    let temp_dir = create_root_file("project(example)\n")?;
    fs::write(temp_dir.join("cmake-tidy.toml"), "not = [valid\n")
        .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .current_dir(&temp_dir)
        .arg("check")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).context("stderr should be valid UTF-8")?;
    assert!(stderr.contains("failed to load configuration"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn check_accepts_direct_cmakelists_input() -> Result<()> {
    let temp_dir = create_root_file("cmake_minimum_required(VERSION 3.30)\nproject(example)\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("check")
        .arg(&cmakelists)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn debug_ast_prints_ast_for_valid_input() -> Result<()> {
    let temp_dir = create_root_file("project(example)\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("debug")
        .arg("ast")
        .arg(&cmakelists)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).context("stdout should be valid UTF-8")?;
    assert!(stdout.contains("CommandInvocation"));
    assert!(stdout.contains("project"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn debug_ast_reports_parse_errors() -> Result<()> {
    let temp_dir = create_root_file("project(example\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("debug")
        .arg("ast")
        .arg(&cmakelists)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).context("stderr should be valid UTF-8")?;
    assert!(stderr.contains("parse errors:"));
    assert!(stderr.contains("expected `)` to close command invocation"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn debug_ast_reports_missing_files() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    let missing = temp_dir.join("Missing.cmake");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("debug")
        .arg("ast")
        .arg(&missing)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).context("stderr should be valid UTF-8")?;
    assert!(stderr.contains("failed to read CMake file"));

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

fn create_root_file(contents: &str) -> Result<PathBuf> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let cmakelists = temp_dir.join("CMakeLists.txt");
    fs::write(&cmakelists, contents)
        .with_context(|| format!("failed to write {}", cmakelists.display()))?;
    Ok(temp_dir)
}

fn unique_temp_dir() -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_nanos();
    let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    Ok(std::env::temp_dir().join(format!(
        "cmake-tidy-check-{}-{timestamp}-{sequence}",
        std::process::id(),
    )))
}
