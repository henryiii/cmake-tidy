use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use cmake_tidy_check::{CheckOptions, Diagnostic, apply_fixes, check_source};
use cmake_tidy_config::{LintConfiguration, MainConfiguration, RuleSelector, load_configuration};

pub(crate) fn run(
    paths: Vec<PathBuf>,
    select: Vec<RuleSelector>,
    ignore: Vec<RuleSelector>,
    fix: bool,
) -> Result<bool> {
    let current_directory = std::env::current_dir().context("failed to read current directory")?;
    let configuration = load_configuration(&current_directory)
        .with_context(|| format!("failed to load configuration from {}", current_directory.display()))?;
    let lint = build_lint_configuration(&configuration.lint, select, ignore);
    let fix_enabled = fix || configuration.main.fix;
    let targets = discover_targets(&paths, &current_directory, &configuration.main)?;
    if targets.is_empty() {
        bail!("no CMake files found");
    }

    let mut found_diagnostics = false;

    for target in targets {
        let source = fs::read_to_string(&target.path)
            .with_context(|| format!("failed to read CMake file: {}", target.path.display()))?;
        let options = CheckOptions {
            project_root: target.project_root,
            function_name_case: configuration.lint.function_name_case,
        };
        let mut diagnostics = filter_diagnostics(check_source(&source, &options).diagnostics, &lint);

        let current_source = if fix_enabled {
            if let Some(fixed) = apply_fixes(&source, &diagnostics) {
                fs::write(&target.path, &fixed)
                    .with_context(|| format!("failed to write fixed file: {}", target.path.display()))?;
                filter_diagnostics(check_source(&fixed, &options).diagnostics, &lint);
                fixed
            } else {
                source
            }
        } else {
            source
        };

        diagnostics = filter_diagnostics(check_source(&current_source, &options).diagnostics, &lint);

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

fn filter_diagnostics(diagnostics: Vec<Diagnostic>, lint: &LintConfiguration) -> Vec<Diagnostic> {
    diagnostics
        .into_iter()
        .filter(|diagnostic| lint.is_rule_enabled(&diagnostic.code.to_string()))
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
    let metadata = fs::metadata(current_path)
        .with_context(|| format!("failed to read file metadata: {}", current_path.display()))?;

    if metadata.is_file() {
        if !is_excluded(current_path, current_directory, main) {
            targets.push(FileTarget {
                path: current_path.to_path_buf(),
                project_root: current_path.file_name().is_some_and(|file_name| file_name == "CMakeLists.txt"),
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

    let entries = fs::read_dir(current_path)
        .with_context(|| format!("failed to read directory: {}", current_path.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", current_path.display()))?;
        let entry_path = entry.path();

        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            collect_targets(input_path, &entry_path, current_directory, main, targets)?;
            continue;
        }

        let is_direct_root_file = current_path == input_path
            && entry_path.file_name().is_some_and(|file_name| file_name == "CMakeLists.txt");
        if is_direct_root_file {
            continue;
        }

        if is_cmake_file(&entry_path) {
            if !is_excluded(&entry_path, current_directory, main) {
                targets.push(FileTarget {
                    path: entry_path,
                    project_root: false,
                });
            }
        }
    }

    Ok(())
}

fn is_cmake_file(path: &Path) -> bool {
    path.file_name().is_some_and(|file_name| file_name == "CMakeLists.txt")
        || path.extension().is_some_and(|extension| extension == "cmake")
}

fn is_excluded(path: &Path, current_directory: &Path, main: &MainConfiguration) -> bool {
    path
        .strip_prefix(current_directory)
        .map_or_else(|_| main.is_path_excluded(path), |relative| main.is_path_excluded(relative))
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
    use super::SourceIndex;
    use cmake_tidy_ast::TextRange;

    #[test]
    fn computes_one_based_locations() {
        let source_index = SourceIndex::new("first()\nsecond()\n");
        assert_eq!(source_index.line_column(TextRange::new(8, 8).start), (2, 1));
    }
}
