use std::fmt;
use std::str::FromStr;

use cmake_tidy_ast::{CommandInvocation, File, Statement, TextRange};
use cmake_tidy_config::{NameCase, RuleSelector};
use cmake_tidy_lexer::{Token, TokenKind};
use cmake_tidy_parser::{ParseError, parse_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CheckOptions {
    pub project_root: bool,
    pub function_name_case: NameCase,
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
    pub fix: Option<Edit>,
}

impl Diagnostic {
    #[must_use]
    pub fn new(code: RuleCode, message: impl Into<String>, range: TextRange) -> Self {
        Self {
            code,
            message: message.into(),
            range,
            fix: None,
        }
    }

    #[must_use]
    pub fn with_fix(mut self, fix: Edit) -> Self {
        self.fix = Some(fix);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: TextRange,
    pub replacement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleCode {
    E001,
    N001,
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
            Self::N001 => "N001",
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
    let noqa = NoqaDirectives::from_tokens(source, &parsed.tokens);
    diagnostics.retain(|diagnostic| !noqa.suppresses(diagnostic));
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
    check_function_name_case(&commands, options.function_name_case, &mut diagnostics);
    check_empty_project_calls(&commands, &mut diagnostics);

    if options.project_root {
        check_project_root_commands(file, &commands, &mut diagnostics);
    }

    diagnostics
}

fn check_function_name_case(
    commands: &[&CommandInvocation],
    naming: NameCase,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for command in commands {
        let expected = match naming {
            NameCase::Lower => command.name.text.to_ascii_lowercase(),
            NameCase::Upper => command.name.text.to_ascii_uppercase(),
        };

        if command.name.text == expected {
            continue;
        }

        let description = match naming {
            NameCase::Lower => "lowercase",
            NameCase::Upper => "uppercase",
        };
        diagnostics.push(
            Diagnostic::new(
                RuleCode::N001,
                format!("function names should use {description} style"),
                command.name.range,
            )
            .with_fix(Edit {
                range: command.name.range,
                replacement: expected,
            }),
        );
    }
}

#[must_use]
pub fn apply_fixes(source: &str, diagnostics: &[Diagnostic]) -> Option<String> {
    let mut edits = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.fix.as_ref())
        .collect::<Vec<_>>();
    if edits.is_empty() {
        return None;
    }

    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));

    let mut output = String::with_capacity(source.len());
    let mut offset = 0;

    for edit in edits {
        if edit.range.start < offset {
            continue;
        }
        output.push_str(&source[offset..edit.range.start]);
        output.push_str(&edit.replacement);
        offset = edit.range.end;
    }

    output.push_str(&source[offset..]);
    (output != source).then_some(output)
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

#[derive(Debug)]
struct NoqaDirectives {
    file: Option<Directive>,
    line_index: LineIndex,
    line_directives: Vec<(usize, Directive)>,
}

impl NoqaDirectives {
    fn from_tokens(source: &str, tokens: &[Token]) -> Self {
        let line_index = LineIndex::new(source);
        let mut directives = Self {
            file: leading_file_directive(tokens),
            line_index,
            line_directives: Vec::new(),
        };

        for token in tokens {
            let TokenKind::Comment(comment) = &token.kind else {
                continue;
            };

            if let Some(directive) = parse_line_directive(comment) {
                directives
                    .line_directives
                    .push((directives.line_index.line_number(token.range.start), directive));
            }
        }

        directives
    }

    fn suppresses(&self, diagnostic: &Diagnostic) -> bool {
        let code = diagnostic.code.to_string();

        if self.file.as_ref().is_some_and(|directive| directive.matches(&code)) {
            return true;
        }

        let line = self.line_index.line_number(diagnostic.range.start);
        self.line_directives.iter().any(|(directive_line, directive)| {
            *directive_line == line && directive.matches(&code)
        })
    }
}

#[derive(Debug)]
struct Directive {
    selectors: Vec<RuleSelector>,
}

impl Directive {
    fn matches(&self, code: &str) -> bool {
        self.selectors
            .iter()
            .any(|selector| selector.matches(code))
    }
}

fn leading_file_directive(tokens: &[Token]) -> Option<Directive> {
    for token in tokens {
        match &token.kind {
            TokenKind::Comment(comment) => {
                if let Some(directive) = parse_file_directive(comment) {
                    return Some(directive);
                }
            }
            TokenKind::Whitespace(_) | TokenKind::Newline => {}
            _ => break,
        }
    }

    None
}

fn parse_file_directive(comment: &str) -> Option<Directive> {
    let body = comment.strip_prefix('#')?.trim();
    parse_noqa_directive(body)
}

fn parse_line_directive(comment: &str) -> Option<Directive> {
    let body = comment.strip_prefix('#')?.trim();
    parse_noqa_directive(body)
}

fn parse_noqa_directive(value: &str) -> Option<Directive> {
    let directive = value.strip_prefix("noqa")?.trim();
    if directive.is_empty() {
        return Some(Directive {
            selectors: vec![RuleSelector::All],
        });
    }

    let selectors = directive.strip_prefix(':')?.split(',').map(str::trim).map(RuleSelector::from_str).collect::<Result<Vec<_>, _>>().ok()?;
    if selectors.is_empty() {
        return None;
    }

    Some(Directive { selectors })
}

#[derive(Debug)]
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (index, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(index + 1);
            }
        }

        Self { line_starts }
    }

    fn line_number(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }
}

#[cfg(test)]
mod tests {
    use cmake_tidy_config::NameCase;

    use super::{CheckOptions, RuleCode, apply_fixes, check_source};

    fn diagnostic_codes(source: &str, options: CheckOptions) -> Vec<RuleCode> {
        check_source(source, &options)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    #[test]
    fn reports_parse_errors() {
        let codes = diagnostic_codes(
            "project(example",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes.len(), 2);
        assert!(codes.contains(&RuleCode::E001));
        assert!(codes.contains(&RuleCode::W301));
    }

    #[test]
    fn reports_missing_root_project_commands() {
        let codes = diagnostic_codes(
            "add_library(example STATIC main.cpp)\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes, vec![RuleCode::W301, RuleCode::W302]);
    }

    #[test]
    fn reports_duplicate_root_commands() {
        let codes = diagnostic_codes(
            "cmake_minimum_required(VERSION 3.30)\ncmake_minimum_required(VERSION 3.31)\nproject(example)\nproject(example-again)\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes, vec![RuleCode::W201, RuleCode::W202]);
    }

    #[test]
    fn reports_empty_project_calls() {
        let codes = diagnostic_codes(
            "cmake_minimum_required(VERSION 3.30)\nproject()\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes, vec![RuleCode::W203]);
    }

    #[test]
    fn skips_root_only_rules_for_non_root_files() {
        let codes = diagnostic_codes(
            "add_subdirectory(src)\n",
            CheckOptions {
                project_root: false,
                function_name_case: NameCase::Lower,
            },
        );
        assert!(codes.is_empty());
    }

    #[test]
    fn line_noqa_suppresses_matching_rule() {
        let codes = diagnostic_codes(
            "project() # noqa: W203\nproject(example)\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert!(!codes.contains(&RuleCode::W203));
        assert!(codes.contains(&RuleCode::W202));
        assert!(codes.contains(&RuleCode::W301));
    }

    #[test]
    fn file_noqa_suppresses_all_rules() {
        let codes = diagnostic_codes(
            "# noqa\nproject()\nproject(example)\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert!(codes.is_empty());
    }

    #[test]
    fn file_noqa_can_target_specific_rules() {
        let codes = diagnostic_codes(
            "# noqa: W301,W202\nproject()\nproject(example)\n",
            CheckOptions {
                project_root: true,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes, vec![RuleCode::W203]);
    }

    #[test]
    fn reports_lowercase_naming_rule() {
        let codes = diagnostic_codes(
            "ADD_LIBRARY(example STATIC main.cpp)\n",
            CheckOptions {
                project_root: false,
                function_name_case: NameCase::Lower,
            },
        );
        assert_eq!(codes, vec![RuleCode::N001]);
    }

    #[test]
    fn reports_uppercase_naming_rule() {
        let codes = diagnostic_codes(
            "add_library(example STATIC main.cpp)\n",
            CheckOptions {
                project_root: false,
                function_name_case: NameCase::Upper,
            },
        );
        assert_eq!(codes, vec![RuleCode::N001]);
    }

    #[test]
    fn applies_naming_fixes() {
        let result = check_source(
            "ADD_LIBRARY(example STATIC main.cpp)\n",
            &CheckOptions {
                project_root: false,
                function_name_case: NameCase::Lower,
            },
        );
        let fixed = apply_fixes("ADD_LIBRARY(example STATIC main.cpp)\n", &result.diagnostics)
            .expect("naming fix should produce an edit");
        assert_eq!(fixed, "add_library(example STATIC main.cpp)\n");
    }
}
