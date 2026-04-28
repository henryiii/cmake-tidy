use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use cmake_tidy_parser::parse_file;

#[derive(Debug, Parser)]
#[command(author, version, about = "CMake linter and formatter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Debug {
        #[command(subcommand)]
        command: DebugSubcommand,
    },
}

#[derive(Debug, Subcommand)]
enum DebugSubcommand {
    Ast { filename: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Debug { command } => match command {
            DebugSubcommand::Ast { filename } => debug_ast(filename),
        },
    }
}

fn debug_ast(filename: PathBuf) -> Result<()> {
    let source = fs::read_to_string(&filename)
        .with_context(|| format!("failed to read CMake file: {}", filename.display()))?;

    let parsed = parse_file(&source);
    println!("{:#?}", parsed.syntax);

    if parsed.errors.is_empty() {
        return Ok(());
    }

    eprintln!("parse errors:");
    for error in &parsed.errors {
        eprintln!(
            "- {} [{}..{}]",
            error.message, error.range.start, error.range.end
        );
    }

    bail!("encountered {} parse error(s)", parsed.errors.len());
}
