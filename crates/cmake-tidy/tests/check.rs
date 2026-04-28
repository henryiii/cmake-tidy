use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

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
    Ok(std::env::temp_dir().join(format!(
        "cmake-tidy-check-{}-{timestamp}",
        std::process::id()
    )))
}
