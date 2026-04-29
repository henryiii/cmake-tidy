mod check;
mod format;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cmake_tidy_config::RuleSelector;
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
        #[arg(long)]
        fix: bool,
        paths: Vec<PathBuf>,
    },
    Format {
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
            fix,
            paths,
        } => check::run(&paths, select, ignore, fix).map(ExitStatus::from_has_diagnostics),
        Command::Format { paths } => format::run(&paths).map(|_| ExitStatus::Success),
        Command::Debug { command } => match command {
            DebugSubcommand::Ast { filename } => debug_ast(&filename),
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

fn debug_ast(filename: &std::path::Path) -> Result<ExitStatus> {
    let source = fs::read_to_string(filename)
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
