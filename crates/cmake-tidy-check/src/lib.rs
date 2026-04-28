use std::fmt;

use cmake_tidy_ast::{CommandInvocation, File, Statement, TextRange};
use cmake_tidy_parser::{ParseError, parse_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckOptions {
    pub project_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: RuleCode,
    pub message: String,
    pub range: TextRange,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: RuleCode, message: impl Into<String>, range: TextRange) -> Self {
        Self {
            code,
            message: message.into(),
            range,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleCode {
    E001,
    W201,
    W202,
    W203,
    W301,
    W302,
}

impl fmt::Display for RuleCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::E001 => "E001",
            Self::W201 => "W201",
            Self::W202 => "W202",
            Self::W203 => "W203",
            Self::W301 => "W301",
            Self::W302 => "W302",
        };

        formatter.write_str(code)
    }
}

#[must_use]
pub fn check_source(source: &str, options: &CheckOptions) -> CheckResult {
    let parsed = parse_file(source);
    let mut diagnostics = Vec::new();

    diagnostics.extend(parsed.errors.iter().map(parse_error_diagnostic));
    diagnostics.extend(check_file(&parsed.syntax, options));
    diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start, diagnostic.range.end, diagnostic.code));

    CheckResult { diagnostics }
}

fn parse_error_diagnostic(error: &ParseError) -> Diagnostic {
    Diagnostic::new(RuleCode::E001, error.message.clone(), error.range)
}

fn check_file(file: &File, options: &CheckOptions) -> Vec<Diagnostic> {
    let commands = file
        .items
        .iter()
        .map(|statement| match statement {
            Statement::Command(command) => command,
        })
        .collect::<Vec<_>>();

    let mut diagnostics = Vec::new();
    check_empty_project_calls(&commands, &mut diagnostics);

    if options.project_root {
        check_project_root_commands(file, &commands, &mut diagnostics);
    }

    diagnostics
}

fn check_empty_project_calls(commands: &[&CommandInvocation], diagnostics: &mut Vec<Diagnostic>) {
    for command in commands {
        if command.name.text.eq_ignore_ascii_case("project") && command.arguments.is_empty() {
            diagnostics.push(Diagnostic::new(
                RuleCode::W203,
                "`project()` should declare at least a project name",
                command.name.range,
            ));
        }
    }
}

fn check_project_root_commands(
    file: &File,
    commands: &[&CommandInvocation],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let cmake_minimum_required = find_commands(commands, "cmake_minimum_required");
    let project_commands = find_commands(commands, "project");

    if cmake_minimum_required.is_empty() {
        diagnostics.push(Diagnostic::new(
            RuleCode::W301,
            "missing `cmake_minimum_required()` in the project root `CMakeLists.txt`",
            file.range,
        ));
    } else {
        for duplicate in &cmake_minimum_required[1..] {
            diagnostics.push(Diagnostic::new(
                RuleCode::W201,
                "duplicate `cmake_minimum_required()` declaration",
                duplicate.name.range,
            ));
        }
    }

    if project_commands.is_empty() {
        diagnostics.push(Diagnostic::new(
            RuleCode::W302,
            "missing `project()` in the project root `CMakeLists.txt`",
            file.range,
        ));
    } else {
        for duplicate in &project_commands[1..] {
            diagnostics.push(Diagnostic::new(
                RuleCode::W202,
                "duplicate `project()` declaration",
                duplicate.name.range,
            ));
        }
    }
}

fn find_commands<'a>(commands: &[&'a CommandInvocation], name: &str) -> Vec<&'a CommandInvocation> {
    commands
        .iter()
        .copied()
        .filter(|command| command.name.text.eq_ignore_ascii_case(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{CheckOptions, RuleCode, check_source};

    fn diagnostic_codes(source: &str, options: CheckOptions) -> Vec<RuleCode> {
        check_source(source, &options)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn reports_parse_errors() {
        let codes = diagnostic_codes("project(example", CheckOptions { project_root: true });
        assert_eq!(codes.len(), 2);
        assert!(codes.contains(&RuleCode::E001));
        assert!(codes.contains(&RuleCode::W301));
    }

    #[test]
    fn reports_missing_root_project_commands() {
        let codes = diagnostic_codes("add_library(example STATIC main.cpp)\n", CheckOptions { project_root: true });
        assert_eq!(codes, vec![RuleCode::W301, RuleCode::W302]);
    }

    #[test]
    fn reports_duplicate_root_commands() {
        let codes = diagnostic_codes(
            "cmake_minimum_required(VERSION 3.30)\ncmake_minimum_required(VERSION 3.31)\nproject(example)\nproject(example-again)\n",
            CheckOptions { project_root: true },
        );
        assert_eq!(codes, vec![RuleCode::W201, RuleCode::W202]);
    }

    #[test]
    fn reports_empty_project_calls() {
        let codes = diagnostic_codes(
            "cmake_minimum_required(VERSION 3.30)\nproject()\n",
            CheckOptions { project_root: true },
        );
        assert_eq!(codes, vec![RuleCode::W203]);
    }

    #[test]
    fn skips_root_only_rules_for_non_root_files() {
        let codes = diagnostic_codes("add_subdirectory(src)\n", CheckOptions { project_root: false });
        assert!(codes.is_empty());
    }
}
