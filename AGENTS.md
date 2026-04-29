# Instructions for `cmake-tidy`

## Build, test, and lint commands

```bash
cargo build
cargo test
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
```

Run the CLI locally with:

```bash
cargo run -p cmake-tidy -- <subcommand> ...
```

Useful targeted test commands:

```bash
# Run one integration test target
cargo test -p cmake-tidy --test check
cargo test -p cmake-tidy --test format

# Run one integration test function by name
cargo test -p cmake-tidy --test check check_select_filters_diagnostics
cargo test -p cmake-tidy --test format format_removes_trailing_spaces

# Run parser / checker fixture suites
cargo test -p cmake-tidy-parser --test fixtures
cargo test -p cmake-tidy-check --test fixtures
```

Coverage commands:

```bash
# Canonical workspace coverage summary
cargo coverage

# Generate an HTML coverage report
cargo coverage-html

# Raw coverage without repo-level exclusions
cargo llvm-cov --workspace --all-features --summary-only
```

- `cargo coverage` and `cargo coverage-html` are defined in `.cargo/config.toml`.
- Those aliases intentionally exclude `coverage_excluded.rs` helper files, which only wrap low-value filesystem and IO error handling.
- Use the raw `cargo llvm-cov` command if you specifically want to include those helper files in the report.

## High-level architecture

This repository is a Rust workspace with separate crates for each stage of the pipeline:

- `cmake-tidy-lexer`: tokenizes CMake source and preserves trivia such as comments, whitespace, and newlines.
- `cmake-tidy-parser`: parses through `TokenSource`, which skips trivia for parsing but keeps the full token list. The parser returns `Parsed<T>` = AST + full tokens + parse errors.
- `cmake-tidy-ast`: syntax node and range types shared across the workspace.
- `cmake-tidy-check`: lint engine. It reparses source, produces `Diagnostic`s, applies `# noqa` suppression from tokens, and can attach autofixes as `Edit`s.
- `cmake-tidy-format`: formatter. It is intentionally still token/line based, not a full AST/layout formatter yet.
- `cmake-tidy-config`: config discovery and normalization for `cmake-tidy.toml`, `.cmake-tidy.toml`, and `pyproject.toml` under `[tool.cmake-tidy]`.
- `cmake-tidy`: CLI crate that wires config loading, path discovery, excludes, per-file ignores, fix application, and exit codes together.

The important project shape is **AST + full token stream**, not a single lossless CST. Both linting and formatting depend on that split:

- lint rules are mostly structural and run from parsed AST plus token-derived suppression data
- formatting currently uses tokenized source plus protected ranges for bracket arguments
- the CLI owns filesystem traversal and decides which files count as project-root `CMakeLists.txt` files for root-only rules

## Key conventions

- Rule selection is Ruff-style everywhere. Use `cmake_tidy_config::RuleSelector` rather than ad hoc rule filtering. Prefixes like `E`, `W3`, exact codes like `W203`, and `ALL` are all first-class.
- `check` merges CLI selectors with config in `crates/cmake-tidy/src/check.rs`: `--select` replaces the configured/default active set, and `--ignore` subtracts from it.
- Autofix is centralized. Rules in `cmake-tidy-check` report diagnostics and optionally attach an `Edit`; `apply_fixes` applies non-overlapping edits after filtering, instead of rules mutating source directly.
- `# noqa` uses plain CMake comments only: file-level suppression must be at the top of the file, and line-level suppression must be on the same line as the reported code.
- Root-only checks (`W201`, `W202`, `W301`, `W302`) are only meant for the discovered root `CMakeLists.txt`, not every nested CMake file.
- `exclude` is top-level config and is applied by the CLI during target discovery for both `check` and `format`.
- `lint.per-file-ignores` should be handled through the config crate and matched against the stable relative path the CLI computes, not raw absolute temp paths.
- Formatter changes must preserve bracket-argument contents verbatim. Current formatter logic protects those ranges before trimming trailing whitespace or collapsing blank lines.
- Tests frequently create temporary directories and invoke the compiled CLI via `env!("CARGO_BIN_EXE_cmake-tidy")`; keep that pattern for end-to-end command behavior.
