use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cmake_tidy_format::format_source;

pub(crate) fn run(paths: Vec<PathBuf>) -> Result<bool> {
    let targets = discover_targets(&paths)?;
    if targets.is_empty() {
        bail!("no CMake files found");
    }

    let mut changed_any = false;

    for path in targets {
        let source = fs::read_to_string(&path)
            .with_context(|| format!("failed to read CMake file: {}", path.display()))?;
        let result = format_source(&source);
        if !result.changed {
            continue;
        }

        fs::write(&path, result.output)
            .with_context(|| format!("failed to write formatted file: {}", path.display()))?;
        changed_any = true;
    }

    Ok(changed_any)
}

fn discover_targets(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut targets = Vec::new();
    for path in paths {
        collect_targets(path, &mut targets)?;
    }

    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn collect_targets(path: &Path, targets: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read file metadata: {}", path.display()))?;

    if metadata.is_file() {
        if is_cmake_file(path) {
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
            collect_targets(&entry_path, targets)?;
        } else if is_cmake_file(&entry_path) {
            targets.push(entry_path);
        }
    }

    Ok(())
}

fn is_cmake_file(path: &Path) -> bool {
    path.file_name().is_some_and(|file_name| file_name == "CMakeLists.txt")
        || path.extension().is_some_and(|extension| extension == "cmake")
}
