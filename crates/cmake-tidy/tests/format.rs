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
        .arg(".")
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

#[test]
fn format_preserves_cmake_format_disabled_regions() -> Result<()> {
    let source = concat!(
        "project(example)\n",
        "# cmake-format: off\n",
        "message (STATUS \"hi\")   \n",
        "\n",
        "\n",
        "# cmake-format: on\n",
        "add_subdirectory(src)   \n",
    );
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
        concat!(
            "project(example)\n",
            "# cmake-format: off\n",
            "message (STATUS \"hi\")   \n",
            "\n",
            "\n",
            "# cmake-format: on\n",
            "add_subdirectory(src)\n",
        )
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_recurses_and_formats_nested_cmake_files() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    let nested_dir = temp_dir.join("src").join("cmake");
    fs::create_dir_all(&nested_dir)
        .with_context(|| format!("failed to create {}", nested_dir.display()))?;
    let nested_file = nested_dir.join("tooling.cmake");
    fs::write(&nested_file, "message (STATUS \"hi\")   \n")
        .with_context(|| format!("failed to write {}", nested_file.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&temp_dir)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&nested_file)
            .with_context(|| format!("failed to read {}", nested_file.display()))?,
        "message(STATUS \"hi\")\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_accepts_direct_cmake_module_input() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    let module = temp_dir.join("tooling.cmake");
    fs::write(&module, "message (STATUS \"hi\")   \n")
        .with_context(|| format!("failed to write {}", module.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&module)
        .output()
        .context("failed to run cmake-tidy")?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&module)
            .with_context(|| format!("failed to read {}", module.display()))?,
        "message(STATUS \"hi\")\n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_accepts_direct_cmakelists_input() -> Result<()> {
    let temp_dir = create_file("message (STATUS \"hi\")   \n")?;
    let cmakelists = temp_dir.join("CMakeLists.txt");

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&cmakelists)
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
fn format_skips_excluded_nested_files_from_config() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    let included_dir = temp_dir.join("src");
    let excluded_dir = temp_dir.join("generated");
    fs::create_dir_all(&included_dir)
        .with_context(|| format!("failed to create {}", included_dir.display()))?;
    fs::create_dir_all(&excluded_dir)
        .with_context(|| format!("failed to create {}", excluded_dir.display()))?;

    let included_file = included_dir.join("tooling.cmake");
    let excluded_file = excluded_dir.join("tooling.cmake");
    fs::write(&included_file, "message (STATUS \"keep\")   \n")
        .with_context(|| format!("failed to write {}", included_file.display()))?;
    fs::write(&excluded_file, "message (STATUS \"skip\")   \n")
        .with_context(|| format!("failed to write {}", excluded_file.display()))?;
    fs::write(
        temp_dir.join("cmake-tidy.toml"),
        format!("exclude = [\"{}\"]\n", excluded_dir.display()),
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
        fs::read_to_string(&included_file)
            .with_context(|| format!("failed to read {}", included_file.display()))?,
        "message(STATUS \"keep\")\n"
    );
    assert_eq!(
        fs::read_to_string(&excluded_file)
            .with_context(|| format!("failed to read {}", excluded_file.display()))?,
        "message (STATUS \"skip\")   \n"
    );

    fs::remove_dir_all(&temp_dir)
        .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
    Ok(())
}

#[test]
fn format_errors_when_no_cmake_files_are_found() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    fs::write(temp_dir.join("notes.txt"), "hello\n")
        .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
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
fn format_errors_for_direct_non_cmake_file_input() -> Result<()> {
    let temp_dir = unique_temp_dir()?;
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;
    let notes = temp_dir.join("notes.txt");
    fs::write(&notes, "hello\n").with_context(|| format!("failed to write {}", notes.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .arg("format")
        .arg(&notes)
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
fn format_reports_invalid_configuration() -> Result<()> {
    let temp_dir = create_file("project(example)\n")?;
    fs::write(temp_dir.join("cmake-tidy.toml"), "not = [valid\n")
        .with_context(|| format!("failed to write {}", temp_dir.display()))?;

    let output = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
        .current_dir(&temp_dir)
        .arg("format")
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
