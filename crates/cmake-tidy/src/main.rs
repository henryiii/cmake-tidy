mod check;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cmake_tidy_config::{LintConfiguration, RuleSelector};
use cmake_tidy_parser::parse_file;

#[derive(Debug, Parser)]
#[command(author, version, about = "CMake linter and formatter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Check {
        #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append)]
        select: Vec<RuleSelector>,
        #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append)]
        ignore: Vec<RuleSelector>,
        paths: Vec<PathBuf>,
    },
    Debug {
        #[command(subcommand)]
        command: DebugSubcommand,
    },
}

#[derive(Debug, Subcommand)]
enum DebugSubcommand {
    Ast { filename: PathBuf },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Check {
            select,
            ignore,
            paths,
        } => check::run(paths, build_lint_configuration(select, ignore))
            .map(ExitStatus::from_has_diagnostics),
        Command::Debug { command } => match command {
            DebugSubcommand::Ast { filename } => debug_ast(filename),
        },
    };

    match result {
        Ok(ExitStatus::Success) => ExitCode::SUCCESS,
        Ok(ExitStatus::Diagnostics) => ExitCode::from(1),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn build_lint_configuration(
    select: Vec<RuleSelector>,
    ignore: Vec<RuleSelector>,
) -> LintConfiguration {
    let mut lint = LintConfiguration::default();
    if !select.is_empty() {
        lint.select = select;
    }
    if !ignore.is_empty() {
        lint.ignore = ignore;
    }
    lint
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitStatus {
    Success,
    Diagnostics,
}

impl ExitStatus {
    const fn from_has_diagnostics(has_diagnostics: bool) -> Self {
        if has_diagnostics {
            Self::Diagnostics
        } else {
            Self::Success
        }
    }
}

fn debug_ast(filename: PathBuf) -> Result<ExitStatus> {
    let source = fs::read_to_string(&filename)
        .with_context(|| format!("failed to read CMake file: {}", filename.display()))?;

    let parsed = parse_file(&source);
    println!("{:#?}", parsed.syntax);

    if parsed.errors.is_empty() {
        return Ok(ExitStatus::Success);
    }

    eprintln!("parse errors:");
    for error in &parsed.errors {
        eprintln!(
            "- {} [{}..{}]",
            error.message, error.range.start, error.range.end
        );
    }

    Ok(ExitStatus::Diagnostics)
}
