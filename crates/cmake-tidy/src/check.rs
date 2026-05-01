use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cmake_tidy_check::{CheckOptions, Diagnostic, apply_fixes, check_source};
use cmake_tidy_config::{LintConfiguration, MainConfiguration, RuleSelector, load_configuration};

use crate::coverage_excluded;

pub fn run(
    paths: &[PathBuf],
    select: Vec<RuleSelector>,
    ignore: Vec<RuleSelector>,
    fix: bool,
) -> Result<bool> {
    let current_directory = std::env::current_dir().context("failed to read current directory")?;
    let configuration = load_configuration(&current_directory).with_context(|| {
        format!(
            "failed to load configuration from {}",
            current_directory.display()
        )
    })?;
    let lint = build_lint_configuration(&configuration.lint, select, ignore);
    let fix_enabled = fix || configuration.main.fix;
    let targets = discover_targets(paths, &current_directory, &configuration.main)?;
    if targets.is_empty() {
        bail!("no CMake files found");
    }

    let mut found_diagnostics = false;

    for target in targets {
        let source = coverage_excluded::read_cmake_file(&target.path)?;
        let options = CheckOptions {
            project_root: target.project_root,
            function_name_case: configuration.lint.function_name_case,
        };
        let relative_path = relative_match_path(&target.path, &current_directory);
        let mut diagnostics = filter_diagnostics(
            check_source(&source, &options).diagnostics,
            &lint,
            &relative_path,
        );

        let current_source = if fix_enabled {
            if let Some(fixed) = apply_fixes(&source, &diagnostics) {
                coverage_excluded::write_fixed_file(&target.path, &fixed)?;
                fixed
            } else {
                source
            }
        } else {
            source
        };

        diagnostics = filter_diagnostics(
            check_source(&current_source, &options).diagnostics,
            &lint,
            &relative_path,
        );

        if diagnostics.is_empty() {
            continue;
        }

        let source_index = SourceIndex::new(&current_source);
        for diagnostic in diagnostics {
            found_diagnostics = true;
            print_diagnostic(&target.path, &source_index, &diagnostic);
        }
    }

    Ok(found_diagnostics)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FileTarget {
    path: PathBuf,
    project_root: bool,
}

fn build_lint_configuration(
    base: &LintConfiguration,
    select: Vec<RuleSelector>,
    ignore: Vec<RuleSelector>,
) -> LintConfiguration {
    let mut lint = base.clone();
    if !select.is_empty() {
        lint.select = select;
    }
    if !ignore.is_empty() {
        lint.ignore = ignore;
    }
    lint
}

fn filter_diagnostics(
    diagnostics: Vec<Diagnostic>,
    lint: &LintConfiguration,
    path: &Path,
) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .filter(|diagnostic| lint.is_rule_enabled_for_path(path, &diagnostic.code.to_string()))
        .collect()
}

fn discover_targets(
    paths: &[PathBuf],
    current_directory: &Path,
    main: &MainConfiguration,
) -> Result<Vec<FileTarget>> {
    let mut targets = Vec::new();

    for path in paths {
        collect_targets(path, path, current_directory, main, &mut targets)?;
    }

    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn collect_targets(
    input_path: &Path,
    current_path: &Path,
    current_directory: &Path,
    main: &MainConfiguration,
    targets: &mut Vec<FileTarget>,
) -> Result<()> {
    let metadata = coverage_excluded::read_metadata(current_path)?;

    if metadata.is_file() {
        if !is_excluded(current_path, current_directory, main) {
            targets.push(FileTarget {
                path: current_path.to_path_buf(),
                project_root: current_path
                    .file_name()
                    .is_some_and(|file_name| file_name == "CMakeLists.txt"),
            });
        }
        return Ok(());
    }

    let root_cmakelists = current_path.join("CMakeLists.txt");
    if root_cmakelists.is_file() && !is_excluded(&root_cmakelists, current_directory, main) {
        targets.push(FileTarget {
            path: root_cmakelists,
            project_root: true,
        });
    }

    let entries = coverage_excluded::read_directory(current_path)?;

    for entry in entries {
        let entry = coverage_excluded::read_directory_entry(entry, current_path)?;
        let entry_path = entry.path();

        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            collect_targets(input_path, &entry_path, current_directory, main, targets)?;
            continue;
        }

        let is_direct_root_file = current_path == input_path
            && entry_path
                .file_name()
                .is_some_and(|file_name| file_name == "CMakeLists.txt");
        if is_direct_root_file {
            continue;
        }

        if is_cmake_file(&entry_path) && !is_excluded(&entry_path, current_directory, main) {
            targets.push(FileTarget {
                path: entry_path,
                project_root: false,
            });
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

fn is_excluded(path: &Path, current_directory: &Path, main: &MainConfiguration) -> bool {
    main.is_path_excluded(path)
        || path
            .strip_prefix(current_directory)
            .is_ok_and(|relative| main.is_path_excluded(relative))
}

fn relative_match_path(path: &Path, current_directory: &Path) -> PathBuf {
    if let Ok(relative) = path.strip_prefix(current_directory) {
        return relative.to_path_buf();
    }

    if let (Ok(canonical_path), Ok(canonical_current_directory)) = (
        std::fs::canonicalize(path),
        std::fs::canonicalize(current_directory),
    ) && let Ok(relative) = canonical_path.strip_prefix(&canonical_current_directory)
    {
        return relative.to_path_buf();
    }

    path.file_name()
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}

fn print_diagnostic(path: &Path, source_index: &SourceIndex, diagnostic: &Diagnostic) {
    let (line, column) = source_index.line_column(diagnostic.range.start);
    println!(
        "{}:{}:{}: {} {}",
        path.display(),
        line,
        column,
        diagnostic.code,
        diagnostic.message
    );
}

#[derive(Debug, Clone)]
struct SourceIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl SourceIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (index, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(index + 1);
            }
        }

        Self {
            line_starts,
            len: source.len(),
        }
    }

    fn line_column(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.len);
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let column = offset - line_start + 1;
        (line_index + 1, column)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::{Context, Result};
    use cmake_tidy_ast::TextRange;
    use cmake_tidy_check::{Diagnostic, RuleCode};
    use cmake_tidy_config::{LintConfiguration, MainConfiguration, RuleSelector};

    use super::{
        SourceIndex, build_lint_configuration, discover_targets, filter_diagnostics, is_excluded,
        relative_match_path,
    };

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn computes_one_based_locations() {
        let source_index = SourceIndex::new("first()\nsecond()\n");
        assert_eq!(source_index.line_column(TextRange::new(8, 8).start), (2, 1));
    }

    #[test]
    fn discovers_root_and_nested_targets() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        let nested_dir = temp_dir.join("cmake");
        fs::create_dir_all(&nested_dir)
            .with_context(|| format!("failed to create {}", nested_dir.display()))?;

        let root_file = temp_dir.join("CMakeLists.txt");
        let nested_file = nested_dir.join("tooling.cmake");
        fs::write(&root_file, "project(example)\n")
            .with_context(|| format!("failed to write {}", root_file.display()))?;
        fs::write(&nested_file, "message(STATUS hi)\n")
            .with_context(|| format!("failed to write {}", nested_file.display()))?;

        let targets = discover_targets(
            std::slice::from_ref(&temp_dir),
            &temp_dir,
            &MainConfiguration::default(),
        )?;

        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|target| target.path == root_file && target.project_root)
        );
        assert!(
            targets
                .iter()
                .any(|target| target.path == nested_file && !target.project_root)
        );

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn relative_match_path_falls_back_to_file_name_for_missing_paths() {
        let current_directory = std::path::Path::new("/tmp/workspace");
        let path = std::path::Path::new("/totally/elsewhere/tooling.cmake");
        assert_eq!(
            relative_match_path(path, current_directory),
            PathBuf::from("tooling.cmake")
        );
    }

    #[test]
    fn relative_match_path_returns_relative_path_when_under_current_directory() {
        let current_directory = std::path::Path::new("/tmp/workspace");
        let path = current_directory.join("cmake").join("tooling.cmake");
        assert_eq!(
            relative_match_path(&path, current_directory),
            PathBuf::from("cmake").join("tooling.cmake")
        );
    }

    #[test]
    fn discovers_direct_cmakelists_file_as_project_root() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;
        let root_file = temp_dir.join("CMakeLists.txt");
        fs::write(&root_file, "project(example)\n")
            .with_context(|| format!("failed to write {}", root_file.display()))?;

        let targets = discover_targets(
            std::slice::from_ref(&root_file),
            &temp_dir,
            &MainConfiguration::default(),
        )?;

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, root_file);
        assert!(targets[0].project_root);

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn discovers_direct_cmake_module_as_non_root_file() -> Result<()> {
        let temp_dir = unique_temp_dir()?;
        fs::create_dir_all(&temp_dir)
            .with_context(|| format!("failed to create {}", temp_dir.display()))?;
        let module_file = temp_dir.join("tooling.cmake");
        fs::write(&module_file, "message(STATUS hi)\n")
            .with_context(|| format!("failed to write {}", module_file.display()))?;

        let targets = discover_targets(
            std::slice::from_ref(&module_file),
            &temp_dir,
            &MainConfiguration::default(),
        )?;

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, module_file);
        assert!(!targets[0].project_root);

        fs::remove_dir_all(&temp_dir)
            .with_context(|| format!("failed to remove {}", temp_dir.display()))?;
        Ok(())
    }

    #[test]
    fn discover_targets_errors_for_missing_path() {
        let temp_dir = std::env::temp_dir();
        let missing = temp_dir.join("cmake-tidy-check-missing-input");
        let error = discover_targets(
            std::slice::from_ref(&missing),
            &temp_dir,
            &MainConfiguration::default(),
        )
        .expect_err("missing path should error");

        assert!(error.to_string().contains("failed to read file metadata"));
    }

    #[test]
    fn build_lint_configuration_overrides_select_and_ignore() {
        let base = LintConfiguration::default();
        let lint = build_lint_configuration(
            &base,
            vec![RuleSelector::prefix("N")],
            vec![RuleSelector::prefix("N001")],
        );

        assert_eq!(lint.select, vec![RuleSelector::prefix("N")]);
        assert_eq!(lint.ignore, vec![RuleSelector::prefix("N001")]);
    }

    #[test]
    fn filter_diagnostics_respects_enabled_rules_for_path() {
        let diagnostics = vec![
            Diagnostic::new(RuleCode::W201, "duplicate", TextRange::new(0, 1)),
            Diagnostic::new(RuleCode::W203, "empty", TextRange::new(1, 2)),
        ];
        let lint = LintConfiguration {
            select: vec![RuleSelector::prefix("W")],
            ignore: vec![RuleSelector::prefix("W201")],
            ..LintConfiguration::default()
        };

        let filtered =
            filter_diagnostics(diagnostics, &lint, std::path::Path::new("CMakeLists.txt"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].code, RuleCode::W203);
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
            "cmake-tidy-check-unit-{}-{timestamp}-{sequence}",
            std::process::id(),
        )))
    }
}
