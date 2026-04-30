use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tower_lsp::lsp_types::Url;

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[test]
fn server_publishes_diagnostics_and_formats_documents() -> Result<()> {
    let workspace_root = unique_temp_dir()?;
    fs::create_dir_all(&workspace_root)
        .with_context(|| format!("failed to create {}", workspace_root.display()))?;
    let document_path = workspace_root.join("CMakeLists.txt");
    fs::write(&document_path, "project(example)\n")
        .with_context(|| format!("failed to write {}", document_path.display()))?;

    let document_uri = Url::from_file_path(&document_path)
        .map_err(|()| anyhow::anyhow!("failed to build file URL"))?;
    let workspace_uri = Url::from_directory_path(&workspace_root)
        .map_err(|()| anyhow::anyhow!("failed to build workspace URL"))?;

    let mut session = ServerSession::spawn()?;

    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "rootUri": workspace_uri,
        }
    }))?;
    let initialize = session.read_until(|message| message.get("id") == Some(&json!(1)))?;
    assert!(initialize.get("result").is_some());

    session.send(&json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    }))?;
    session.send(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": document_uri,
                "languageId": "cmake",
                "version": 1,
                "text": "project()   \nproject(example)\n"
            }
        }
    }))?;

    let diagnostics = session.read_until(|message| {
        message.get("method") == Some(&json!("textDocument/publishDiagnostics"))
    })?;
    let diagnostics = diagnostics["params"]["diagnostics"]
        .as_array()
        .context("diagnostics payload should be an array")?;
    let codes = diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic["code"].as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"W203"));
    assert!(codes.contains(&"W202"));
    assert!(codes.contains(&"W301"));
    assert!(!codes.contains(&"W302"));

    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": { "uri": document_uri },
            "options": {
                "insertSpaces": true,
                "tabSize": 4
            }
        }
    }))?;
    let formatting = session.read_until(|message| message.get("id") == Some(&json!(2)))?;
    let edits = formatting["result"]
        .as_array()
        .context("formatting result should be an array")?;
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0]["newText"].as_str(),
        Some("project()\nproject(example)\n")
    );

    session.send(&json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "shutdown"
    }))?;
    let shutdown = session.read_until(|message| message.get("id") == Some(&json!(3)))?;
    assert_eq!(shutdown["id"], json!(3));
    assert!(shutdown.get("error").is_none(), "{shutdown}");

    session.send(&json!({
        "jsonrpc": "2.0",
        "method": "exit"
    }))?;
    session.finish()?;

    fs::remove_dir_all(&workspace_root)
        .with_context(|| format!("failed to remove {}", workspace_root.display()))?;
    Ok(())
}

struct ServerSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl ServerSession {
    fn spawn() -> Result<Self> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cmake-tidy"))
            .arg("server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start cmake-tidy server")?;
        let stdin = child.stdin.take().context("server stdin should be piped")?;
        let stdout = child
            .stdout
            .take()
            .context("server stdout should be piped")?;
        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
        })
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let payload =
            serde_json::to_vec(message).context("failed to serialize JSON-RPC message")?;
        let stdin = self
            .stdin
            .as_mut()
            .context("server stdin should still be available")?;
        write!(stdin, "Content-Length: {}\r\n\r\n", payload.len())
            .context("failed to write LSP header")?;
        stdin
            .write_all(&payload)
            .context("failed to write LSP payload")?;
        stdin.flush().context("failed to flush LSP payload")
    }

    fn read_until<F>(&mut self, predicate: F) -> Result<Value>
    where
        F: Fn(&Value) -> bool,
    {
        loop {
            let message = read_message(&mut self.stdout)?;
            if predicate(&message) {
                return Ok(message);
            }
        }
    }

    fn finish(mut self) -> Result<()> {
        drop(self.stdin.take());
        let status = self
            .child
            .wait()
            .context("failed to wait for cmake-tidy server")?;
        if !status.success() {
            bail!("cmake-tidy server exited with {status}");
        }
        Ok(())
    }
}

fn read_message(stdout: &mut BufReader<ChildStdout>) -> Result<Value> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .context("failed to read LSP header line")?;
        if line == "\r\n" {
            break;
        }

        let Some((name, value)) = line.split_once(':') else {
            bail!("invalid LSP header line: {line:?}");
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length header")?,
            );
        }
    }

    let content_length = content_length.context("missing Content-Length header")?;
    let mut payload = vec![0; content_length];
    stdout
        .read_exact(&mut payload)
        .context("failed to read LSP payload")?;
    serde_json::from_slice(&payload).context("failed to decode LSP payload")
}

fn unique_temp_dir() -> Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_nanos();
    let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    Ok(std::env::temp_dir().join(format!(
        "cmake-tidy-server-{}-{timestamp}-{sequence}",
        std::process::id(),
    )))
}
