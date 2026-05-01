use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cmake_tidy_config::load_configuration;
use cmake_tidy_format::format_source_with_options;

use crate::coverage_excluded;

pub fn run(paths: &[PathBuf]) -> Result<bool> {
    let current_directory = std::env::current_dir().context("failed to read current directory")?;
    let configuration = load_configuration(&current_directory).with_context(|| {
        format!(
            "failed to load configuration from {}",
            current_directory.display()
        )
    })?;
    let targets = discover_targets(paths, &current_directory, &configuration.main)?;
    if targets.is_empty() {
        bail!("no CMake files found");
    }

    let mut changed_any = false;

    for path in targets {
        let source = coverage_excluded::read_cmake_file(&path)?;
        let result = format_source_with_options(&source, &configuration.format);
        if !result.changed {
            continue;
        }

        coverage_excluded::write_formatted_file(&path, result.output)?;
        changed_any = true;
    }

    Ok(changed_any)
}

fn discover_targets(
    paths: &[PathBuf],
    current_directory: &Path,
    main: &cmake_tidy_config::MainConfiguration,
) -> Result<Vec<PathBuf>> {
    let mut targets = Vec::new();
    for path in paths {
        collect_targets(path, current_directory, main, &mut targets)?;
    }

    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn collect_targets(
    path: &Path,
    current_directory: &Path,
    main: &cmake_tidy_config::MainConfiguration,
    targets: &mut Vec<PathBuf>,
) -> Result<()> {
    let metadata = coverage_excluded::read_metadata(path)?;

    if metadata.is_file() {
        if is_cmake_file(path) && !is_excluded(path, current_directory, main) {
            targets.push(path.to_path_buf());
        }
        return Ok(());
    }

    for entry in coverage_excluded::read_directory(path)? {
        let entry = coverage_excluded::read_directory_entry(entry, path)?;
        let entry_path = entry.path();

        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            collect_targets(&entry_path, current_directory, main, targets)?;
        } else if is_cmake_file(&entry_path) && !is_excluded(&entry_path, current_directory, main) {
            targets.push(entry_path);
        }
    }

    Ok(())
}

fn is_cmake_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|file_name| file_name == "CMakeLists.txt")
        || path
            .extension()
            .is_some_and(|extension| extension == "cmake")
}

fn is_excluded(
    path: &Path,
    current_directory: &Path,
    main: &cmake_tidy_config::MainConfiguration,
) -> bool {
    if main.is_path_excluded(path) {
        return true;
    }

    if path
        .strip_prefix(current_directory)
        .is_ok_and(|relative| main.is_path_excluded(relative))
    {
        return true;
    }

    if let (Ok(canonical_path), Ok(canonical_current_directory)) = (
        std::fs::canonicalize(path),
        std::fs::canonicalize(current_directory),
    ) && canonical_path
        .strip_prefix(&canonical_current_directory)
        .is_ok_and(|relative| main.is_path_excluded(relative))
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::{Context, Result};
    use cmake_tidy_config::MainConfiguration;

    use super::{discover_targets, is_cmake_file, is_excluded};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn discovers_direct_cmake_file_inputs() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;
        let cmake_file = temp_dir.join("tooling.cmake");
        fs::write(&cmake_file, "message(STATUS hi)\n")
            .with_context(|| format!("failed to write {}", cmake_file.display()))?;

        let targets = discover_targets(
            std::slice::from_ref(&cmake_file),
            &temp_dir,
            &MainConfiguration::default(),
        )?;

        assert_eq!(targets, vec![cmake_file]);

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn deduplicates_targets_when_directory_and_file_overlap() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;
        let cmakelists = temp_dir.join("CMakeLists.txt");
        fs::write(&cmakelists, "project(example)\n")
            .with_context(|| format!("failed to write {}", cmakelists.display()))?;

        let targets = discover_targets(
            &[temp_dir.clone(), cmakelists.clone()],
            &temp_dir,
            &MainConfiguration::default(),
        )?;

        assert_eq!(targets, vec![cmakelists]);

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn recognizes_cmake_file_names() {
        assert!(is_cmake_file(std::path::Path::new("CMakeLists.txt")));
        assert!(is_cmake_file(std::path::Path::new("tooling.cmake")));
        assert!(!is_cmake_file(std::path::Path::new("notes.txt")));
    }

    #[test]
    fn discovers_no_targets_for_excluded_file_input() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;
        let excluded_dir = temp_dir.join("generated");
        fs::create_dir_all(&excluded_dir)
            .with_context(|| format!("failed to create {}", excluded_dir.display()))?;
        let cmake_file = excluded_dir.join("tooling.cmake");
        fs::write(&cmake_file, "message(STATUS hi)\n")
            .with_context(|| format!("failed to write {}", cmake_file.display()))?;

        let targets = discover_targets(
            std::slice::from_ref(&cmake_file),
            &temp_dir,
            &MainConfiguration {
                exclude: vec![PathBuf::from("generated")],
                ..MainConfiguration::default()
            },
        )?;

        assert!(targets.is_empty());

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn discover_targets_errors_for_missing_path() {
        let temp_dir = std::env::temp_dir();
        let missing = temp_dir.join("cmake-tidy-format-missing-input");
        let error = discover_targets(
            std::slice::from_ref(&missing),
            &temp_dir,
            &MainConfiguration::default(),
        )
        .expect_err("missing path should error");

        assert!(error.to_string().contains("failed to read file metadata"));
    }

    #[test]
    fn excludes_match_relative_paths() {
        let current_directory = std::path::Path::new("/workspace");
        let path = current_directory.join("generated").join("tooling.cmake");
        let configuration = MainConfiguration {
            exclude: vec![PathBuf::from("generated")],
            ..MainConfiguration::default()
        };

        assert!(is_excluded(&path, current_directory, &configuration));
        assert!(!is_excluded(
            &current_directory.join("src").join("tooling.cmake"),
            current_directory,
            &configuration,
        ));
    }

    fn unique_temp_dir() -> Result<PathBuf> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before UNIX_EPOCH")?
            .as_nanos();
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        Ok(std::env::temp_dir().join(format!(
            "cmake-tidy-format-unit-{}-{timestamp}-{sequence}",
            std::process::id(),
        )))
    }
}
