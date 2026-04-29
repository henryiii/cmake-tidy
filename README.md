# cmake-tidy

`cmake-tidy` is an in-progress Rust tool for linting and formatting CMake files.

The project is currently split into separate crates for parsing, checking, formatting, and configuration, with a Ruff-inspired architecture of:

- lossless tokenization
- a semantic AST
- separate lint and format pipelines

## Current commands

### `cmake-tidy debug ast <filename>`

Parses a CMake file and prints the current AST.

### `cmake-tidy check [OPTIONS] <paths...>`

Runs lint checks on `CMakeLists.txt` and `.cmake` files.

Current rules:

| Code | Description |
| --- | --- |
| `E001` | Parse error |
| `N001` | Command/function name case does not match the configured naming style |
| `W201` | Duplicate `cmake_minimum_required()` |
| `W202` | Duplicate `project()` |
| `W203` | Empty `project()` |
| `W301` | Missing `cmake_minimum_required()` |
| `W302` | Missing `project()` |

Supported selector flags:

- `--select E,W301`
- `--ignore W203`
- `--fix`

Selector behavior is Ruff-style:

- `E` selects all `E***` rules
- `N` selects all `N***` rules
- `W3` selects all `W3**` rules
- exact codes like `W203` are supported
- `ALL` selects all rules
- `--select` replaces the default selection when provided
- `--ignore` subtracts from the active set
- `--fix` applies available autofixes in place

Default lint selection:

```toml
["E", "W"]
```

### `cmake-tidy format <paths...>`

Formats `CMakeLists.txt` and `.cmake` files in place.

Current formatting behavior:

- removes trailing spaces and tabs
- enforces a final newline
- trims trailing blank lines at EOF
- normalizes spacing between command names and `(`
- collapses consecutive blank lines
- preserves bracket-argument contents verbatim
- honors `# cmake-format: off` / `# cmake-format: on` regions verbatim

## Configuration

Configuration is discovered in this order:

1. `cmake-tidy.toml`
2. `.cmake-tidy.toml`
3. `pyproject.toml` under `[tool.cmake-tidy]`

## Configuration schema

The current configuration has three sections:

- top-level main settings
- `[lint]`
- `[format]`

### Main settings

```toml
exclude = ["build", "third_party"]
```

`exclude` filters paths by prefix.

### Lint settings

```toml
fix = false

[lint]
select = ["E", "W"]
ignore = ["W203"]
function-name-case = "lower"

[lint.per-file-ignores]
"tests/**/CMakeLists.txt" = ["W301"]
"vendor/*.cmake" = ["ALL"]
```

### Format settings

```toml
[format]
final-newline = true
max-blank-lines = 1
space-before-paren = false
```

Current format settings:

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `final-newline` | `bool` | `true` | Ensure files end with a newline |
| `max-blank-lines` | integer | `1` | Maximum number of consecutive blank lines |
| `space-before-paren` | `bool` | `false` | Use `message (...)` instead of `message(...)` |

## `pyproject.toml` example

```toml
[tool.cmake-tidy]
exclude = ["build", "vendor"]
fix = false

[tool.cmake-tidy.lint]
select = ["E", "W"]
ignore = ["W203"]
function-name-case = "lower"

[tool.cmake-tidy.lint.per-file-ignores]
"tests/**/CMakeLists.txt" = ["W301"]
"vendor/*.cmake" = ["ALL"]

[tool.cmake-tidy.format]
final-newline = true
max-blank-lines = 1
space-before-paren = false
```

## `noqa` support

Suppressions use plain `# noqa`.

Line-level:

```cmake
project() # noqa: W203
```

File-level suppression is enabled by putting `# noqa` at the top of the file:

```cmake
# noqa
project()
project(example)
```

You can also suppress only specific rules for the whole file:

```cmake
# noqa: W201,W301
```

## Formatter disable regions

`cmake-tidy format` supports `cmake-format`-compatible local disable markers:

```cmake
# cmake-format: off
message (STATUS "leave this spacing alone")   
# cmake-format: on
```

Code inside a disabled region is preserved verbatim. Formatting resumes after the matching
`# cmake-format: on`. If `off` is unmatched, formatting stays disabled until end of file.

## Notes

- Root-only project checks are applied to the discovered root `CMakeLists.txt`.
- Bracket arguments are intentionally preserved verbatim during formatting for now.
- The naming rule family is opt-in; it is not selected by default because default lint selection remains `["E", "W"]`.
- `lint.per-file-ignores` uses Ruff-style pattern-to-selector mappings.
- Use `cargo coverage` for the canonical workspace coverage summary and `cargo coverage-html` for an HTML report.
- The coverage aliases intentionally exclude `coverage_excluded.rs` helper files that only wrap low-value filesystem/error handling paths.
- The architecture notes for longer-term design live in [`ARCHETECTURE.md`](ARCHETECTURE.md).
