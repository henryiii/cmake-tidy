use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use cmake_tidy_ast::TextRange;
use cmake_tidy_check::{CheckOptions, Diagnostic as CheckDiagnostic, RuleCode, check_source};
use cmake_tidy_config::{Configuration, load_configuration};
use cmake_tidy_format::format_source_with_options;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error as JsonRpcError, ErrorCode, Result as JsonResult};
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    InitializeParams, InitializeResult, InitializedParams, MessageType, OneOf, Position, Range,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextEdit, Url, WorkspaceFolder,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

#[derive(Debug, Default)]
struct ServerState {
    workspace_root: Option<PathBuf>,
    documents: HashMap<Url, String>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    state: Arc<RwLock<ServerState>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(ServerState::default())),
        }
    }

    async fn set_workspace_root(&self, workspace_root: Option<PathBuf>) {
        self.state.write().await.workspace_root = workspace_root;
    }

    async fn store_document(&self, uri: Url, text: String) {
        self.state.write().await.documents.insert(uri, text);
    }

    async fn remove_document(&self, uri: &Url) {
        self.state.write().await.documents.remove(uri);
    }

    async fn document_text(&self, uri: &Url) -> Result<String> {
        let text = self.state.read().await.documents.get(uri).cloned();
        if let Some(text) = text {
            return Ok(text);
        }

        let path = file_path_from_uri(uri)?;
        std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read document {}", path.display()))
    }

    async fn workspace_root(&self) -> Result<PathBuf> {
        let workspace_root = self.state.read().await.workspace_root.clone();
        if let Some(path) = workspace_root {
            return Ok(path);
        }

        std::env::current_dir().context("failed to read current directory")
    }

    async fn publish_document_diagnostics(&self, uri: Url, text: &str) {
        match self.analyze_document(&uri, text).await {
            Ok(diagnostics) => {
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
            }
            Err(error) => {
                self.client
                    .publish_diagnostics(uri.clone(), Vec::new(), None)
                    .await;
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("failed to analyze {uri}: {error:#}"),
                    )
                    .await;
            }
        }
    }

    async fn analyze_document(&self, uri: &Url, text: &str) -> Result<Vec<LspDiagnostic>> {
        let workspace_root = self.workspace_root().await?;
        let file_path = file_path_from_uri(uri)?;
        let configuration = load_configuration(&workspace_root).with_context(|| {
            format!(
                "failed to load configuration from {}",
                workspace_root.display()
            )
        })?;

        if is_excluded(&file_path, &workspace_root, &configuration) {
            return Ok(Vec::new());
        }

        let relative_path = relative_match_path(&file_path, &workspace_root);
        let options = CheckOptions {
            project_root: is_workspace_root_cmakelists(&file_path, &workspace_root),
            function_name_case: configuration.lint.function_name_case,
        };

        let index = PositionIndex::new(text);
        Ok(check_source(text, &options)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| {
                configuration
                    .lint
                    .is_rule_enabled_for_path(&relative_path, &diagnostic.code.to_string())
            })
            .map(|diagnostic| to_lsp_diagnostic(&index, diagnostic))
            .collect())
    }

    async fn format_document(&self, uri: &Url) -> Result<Option<Vec<TextEdit>>> {
        let workspace_root = self.workspace_root().await?;
        let file_path = file_path_from_uri(uri)?;
        let configuration = load_configuration(&workspace_root).with_context(|| {
            format!(
                "failed to load configuration from {}",
                workspace_root.display()
            )
        })?;

        if is_excluded(&file_path, &workspace_root, &configuration) {
            return Ok(None);
        }

        let source = self.document_text(uri).await?;
        let result = format_source_with_options(&source, &configuration.format);
        if !result.changed {
            return Ok(None);
        }

        let index = PositionIndex::new(&source);
        Ok(Some(vec![TextEdit {
            range: Range::new(Position::new(0, 0), index.position(source.len())),
            new_text: result.output,
        }]))
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> JsonResult<InitializeResult> {
        self.set_workspace_root(extract_workspace_root(&params))
            .await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "cmake-tidy".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "cmake-tidy LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> JsonResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.store_document(uri.clone(), text.clone()).await;
        self.publish_document_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };

        let uri = params.text_document.uri;
        let text = change.text;
        self.store_document(uri.clone(), text.clone()).await;
        self.publish_document_diagnostics(uri, &text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.remove_document(&uri).await;
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> JsonResult<Option<Vec<TextEdit>>> {
        self.format_document(&params.text_document.uri)
            .await
            .map_err(|error| jsonrpc_error(&error))
    }
}

pub fn run() -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?;
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(Backend::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(())
}

fn extract_workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .and_then(|folders| {
            folders
                .iter()
                .find_map(|folder| workspace_folder_path(folder).ok())
        })
        .or_else(|| {
            params
                .root_uri
                .as_ref()
                .and_then(|uri| uri.to_file_path().ok())
        })
}

fn workspace_folder_path(folder: &WorkspaceFolder) -> Result<PathBuf> {
    folder
        .uri
        .to_file_path()
        .map_err(|()| anyhow!("workspace folder URI must use the file scheme"))
}

fn file_path_from_uri(uri: &Url) -> Result<PathBuf> {
    uri.to_file_path()
        .map_err(|()| anyhow!("document URI must use the file scheme: {uri}"))
}

fn is_workspace_root_cmakelists(path: &Path, workspace_root: &Path) -> bool {
    path.file_name()
        .is_some_and(|file_name| file_name == "CMakeLists.txt")
        && path
            .strip_prefix(workspace_root)
            .is_ok_and(|relative| relative == Path::new("CMakeLists.txt"))
}

fn is_excluded(path: &Path, workspace_root: &Path, configuration: &Configuration) -> bool {
    path.strip_prefix(workspace_root).map_or_else(
        |_| configuration.main.is_path_excluded(path),
        |relative| configuration.main.is_path_excluded(relative),
    )
}

fn relative_match_path(path: &Path, workspace_root: &Path) -> PathBuf {
    path.strip_prefix(workspace_root).map_or_else(
        |_| {
            path.file_name()
                .map_or_else(|| path.to_path_buf(), PathBuf::from)
        },
        PathBuf::from,
    )
}

fn to_lsp_diagnostic(index: &PositionIndex, diagnostic: CheckDiagnostic) -> LspDiagnostic {
    let severity = if diagnostic.code == RuleCode::E001 {
        Some(DiagnosticSeverity::ERROR)
    } else {
        Some(DiagnosticSeverity::WARNING)
    };

    LspDiagnostic {
        range: index.range(diagnostic.range),
        severity,
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            diagnostic.code.to_string(),
        )),
        source: Some("cmake-tidy".to_owned()),
        message: diagnostic.message,
        ..LspDiagnostic::default()
    }
}

fn jsonrpc_error(error: &anyhow::Error) -> JsonRpcError {
    JsonRpcError {
        code: ErrorCode::InternalError,
        message: error.to_string().into(),
        data: None,
    }
}

#[derive(Debug, Clone)]
struct PositionIndex {
    source: String,
    line_starts: Vec<usize>,
}

impl PositionIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (index, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(index + 1);
            }
        }

        Self {
            source: source.to_owned(),
            line_starts,
        }
    }

    fn range(&self, range: TextRange) -> Range {
        Range::new(self.position(range.start), self.position(range.end))
    }

    fn position(&self, offset: usize) -> Position {
        let offset = clamp_char_boundary(&self.source, offset.min(self.source.len()));
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let character = utf16_code_units(&self.source[line_start..offset]);
        Position::new(lsp_u32(line_index), lsp_u32(character))
    }
}

const fn clamp_char_boundary(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn lsp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn utf16_code_units(text: &str) -> usize {
    text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use cmake_tidy_ast::TextRange;
    use tower_lsp::lsp_types::{DiagnosticSeverity, Position};

    use super::{
        PositionIndex, is_workspace_root_cmakelists, relative_match_path, to_lsp_diagnostic,
    };

    #[test]
    fn detects_only_workspace_root_cmakelists() {
        let workspace_root = std::path::Path::new("/workspace");
        assert!(is_workspace_root_cmakelists(
            &workspace_root.join("CMakeLists.txt"),
            workspace_root,
        ));
        assert!(!is_workspace_root_cmakelists(
            &workspace_root.join("src").join("CMakeLists.txt"),
            workspace_root,
        ));
    }

    #[test]
    fn relative_match_path_uses_workspace_relative_path() {
        let workspace_root = std::path::Path::new("/workspace");
        assert_eq!(
            relative_match_path(
                &workspace_root.join("cmake").join("tooling.cmake"),
                workspace_root
            ),
            std::path::PathBuf::from("cmake").join("tooling.cmake")
        );
    }

    #[test]
    fn converts_offsets_to_utf16_positions() {
        let source = "é\n😀x\n";
        let index = PositionIndex::new(source);
        assert_eq!(index.position(0), Position::new(0, 0));
        assert_eq!(index.position("é\n".len()), Position::new(1, 0));
        assert_eq!(index.position("é\n😀".len()), Position::new(1, 2));
    }

    #[test]
    fn maps_check_diagnostics_to_lsp_diagnostics() {
        let index = PositionIndex::new("project(\n");
        let diagnostic = to_lsp_diagnostic(
            &index,
            cmake_tidy_check::Diagnostic::new(
                cmake_tidy_check::RuleCode::E001,
                "parse error",
                TextRange::new(0, 7),
            ),
        );
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            diagnostic.code,
            Some(tower_lsp::lsp_types::NumberOrString::String(
                "E001".to_owned(),
            ))
        );
    }
}
