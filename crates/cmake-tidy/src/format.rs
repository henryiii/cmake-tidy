use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cmake_tidy_config::load_configuration;
use cmake_tidy_format::format_source_with_options;

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
        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read CMake file: {}", path.display()))?;
        let result = format_source_with_options(&source, &configuration.format);
        if !result.changed {
            continue;
        }

        fs::write(&path, result.output)
            .with_context(|| format!("failed to write formatted file: {}", path.display()))?;
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
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read file metadata: {}", path.display()))?;

    if metadata.is_file() {
        if is_cmake_file(path) && !is_excluded(path, current_directory, main) {
            targets.push(path.to_path_buf());
        }
        return Ok(());
    }

    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read directory: {}", path.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", path.display()))?;
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
    path.strip_prefix(current_directory).map_or_else(
        |_| main.is_path_excluded(path),
        |relative| main.is_path_excluded(relative),
    )
}
