use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[test]
fn format_removes_trailing_spaces() -> Result<()> {
    let temp_dir = create_file("project(example)   \nadd_subdirectory(src)\t\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        "project(example)\nadd_subdirectory(src)\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_preserves_multiline_string_contents() -> Result<()> {
    let source = "message([=[\nfirst line    \nsecond line\t\n]=])\n";
    let temp_dir = create_file(source)?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        source
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_removes_space_before_paren_and_trims_eof_blank_lines() -> Result<()> {
    let temp_dir = create_file("message (STATUS \"hi\")\n\n\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        "message(STATUS \"hi\")\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_can_preserve_space_before_paren_via_config() -> Result<()> {
    let temp_dir = create_file("message(STATUS \"hi\")\n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");
    fs::write(
        temp_dir.join("cmake-tidy.toml"),
        "[format]\nspace-before-paren = true\n",
    )
    .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .current_dir(&temp_dir)
        .arg("format")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&cmakelists)
            .with_context(|| format!("failed to read {}", cmakelists.display()))?,
        "message (STATUS \"hi\")\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

fn create_file(contents: &str) -> Result<PathBuf> {
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
        "cmake-tidy-format-{}-{timestamp}-{sequence}",
        std::process::id(),
    )))
}
