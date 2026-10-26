//! Language server for Nand to Tetris `.hdl` and `.tst` files.
//!
//! Every folder containing an open document is treated as a project
//! directory: chips resolve against `.hdl` files in that folder (including
//! unsaved editor buffers) and then the built-in chip library.

mod convert;
mod hdl;
mod semantic_tokens;
mod tst;
mod workspace;

use std::error::Error;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types as lsp;
use lsp_types::Url;
use lsp_types::notification::{self as notif, Notification as _};
use lsp_types::request::{self as req, Request as _};
use serde_json::Value;

use crate::workspace::{DocKind, Workspace};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Runs the server over stdin/stdout until the client shuts it down.
pub fn run_stdio() -> Result<()> {
    let (connection, io_threads) = Connection::stdio();
    run(connection)?;
    io_threads.join()?;
    Ok(())
}

/// Runs the server on an established connection.
pub fn run(connection: Connection) -> Result<()> {
    let capabilities = serde_json::to_value(capabilities())?;
    let params: lsp::InitializeParams =
        serde_json::from_value(connection.initialize(capabilities)?)?;
    let snippets = params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|t| t.completion.as_ref())
        .and_then(|c| c.completion_item.as_ref())
        .and_then(|i| i.snippet_support)
        .unwrap_or(false);
    let mut server = Server {
        connection,
        workspace: Workspace::new(),
        snippets,
    };
    server.main_loop()
}

fn capabilities() -> lsp::ServerCapabilities {
    lsp::ServerCapabilities {
        text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
            lsp::TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(lsp::TextDocumentSyncKind::FULL),
                save: Some(lsp::TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),
        hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
        definition_provider: Some(lsp::OneOf::Left(true)),
        references_provider: Some(lsp::OneOf::Left(true)),
        document_highlight_provider: Some(lsp::OneOf::Left(true)),
        document_symbol_provider: Some(lsp::OneOf::Left(true)),
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        completion_provider: Some(lsp::CompletionOptions {
            trigger_characters: Some(vec!["(".into(), ",".into(), "=".into(), " ".into()]),
            ..Default::default()
        }),
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        semantic_tokens_provider: Some(
            lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp::SemanticTokensOptions {
                    legend: semantic_tokens::legend(),
                    full: Some(lsp::SemanticTokensFullOptions::Bool(true)),
                    range: None,
                    ..Default::default()
                },
            ),
        ),
        ..Default::default()
    }
}

struct Server {
    connection: Connection,
    workspace: Workspace,
    snippets: bool,
}

impl Server {
    fn main_loop(&mut self) -> Result<()> {
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.request(request)?;
                }
                Message::Notification(notification) => self.notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn send(&self, message: Message) -> Result<()> {
        self.connection.sender.send(message)?;
        Ok(())
    }

    fn respond(
        &self,
        id: RequestId,
        result: std::result::Result<Value, (ErrorCode, String)>,
    ) -> Result<()> {
        let response = match result {
            Ok(value) => Response::new_ok(id, value),
            Err((code, message)) => Response::new_err(id, code as i32, message),
        };
        self.send(Message::Response(response))
    }

    fn request(&mut self, request: Request) -> Result<()> {
        let id = request.id.clone();
        let result = self.dispatch(request);
        self.respond(id, result)
    }

    fn dispatch(&mut self, request: Request) -> std::result::Result<Value, (ErrorCode, String)> {
        fn params<R: req::Request>(
            request: Request,
        ) -> std::result::Result<R::Params, (ErrorCode, String)> {
            serde_json::from_value(request.params)
                .map_err(|e| (ErrorCode::InvalidParams, e.to_string()))
        }
        fn ok<T: serde::Serialize>(value: T) -> std::result::Result<Value, (ErrorCode, String)> {
            serde_json::to_value(value).map_err(|e| (ErrorCode::InternalError, e.to_string()))
        }
        let ws = &mut self.workspace;
        let kind = |ws: &Workspace, uri: &Url| ws.docs.get(uri).map(|d| d.kind);

        match request.method.as_str() {
            req::HoverRequest::METHOD => {
                let p = params::<req::HoverRequest>(request)?;
                let uri = p.text_document_position_params.text_document.uri;
                let position = p.text_document_position_params.position;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::hover(ws, &uri, position),
                    Some(DocKind::Tst) => tst::hover(ws, &uri, position),
                    _ => None,
                })
            }
            req::GotoDefinition::METHOD => {
                let p = params::<req::GotoDefinition>(request)?;
                let uri = p.text_document_position_params.text_document.uri;
                let position = p.text_document_position_params.position;
                let location = match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::definition(ws, &uri, position),
                    Some(DocKind::Tst) => tst::definition(ws, &uri, position),
                    _ => None,
                };
                ok(location.map(lsp::GotoDefinitionResponse::Scalar))
            }
            req::References::METHOD => {
                let p = params::<req::References>(request)?;
                let uri = p.text_document_position.text_document.uri;
                let position = p.text_document_position.position;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => {
                        hdl::references(ws, &uri, position, p.context.include_declaration)
                    }
                    _ => Vec::new(),
                })
            }
            req::DocumentHighlightRequest::METHOD => {
                let p = params::<req::DocumentHighlightRequest>(request)?;
                let uri = p.text_document_position_params.text_document.uri;
                let position = p.text_document_position_params.position;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::highlights(ws, &uri, position),
                    _ => Vec::new(),
                })
            }
            req::DocumentSymbolRequest::METHOD => {
                let p = params::<req::DocumentSymbolRequest>(request)?;
                let uri = p.text_document.uri;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => Some(lsp::DocumentSymbolResponse::Nested(
                        hdl::document_symbols(ws, &uri),
                    )),
                    _ => None,
                })
            }
            req::PrepareRenameRequest::METHOD => {
                let p = params::<req::PrepareRenameRequest>(request)?;
                let uri = p.text_document.uri;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::prepare_rename(ws, &uri, p.position),
                    _ => None,
                })
            }
            req::Rename::METHOD => {
                let p = params::<req::Rename>(request)?;
                let uri = p.text_document_position.text_document.uri;
                let position = p.text_document_position.position;
                match kind(ws, &uri) {
                    Some(DocKind::Hdl) => match hdl::rename(ws, &uri, position, &p.new_name) {
                        Ok(edit) => ok(edit),
                        Err(message) => Err((ErrorCode::RequestFailed, message)),
                    },
                    _ => ok(None::<lsp::WorkspaceEdit>),
                }
            }
            req::Completion::METHOD => {
                let p = params::<req::Completion>(request)?;
                let uri = p.text_document_position.text_document.uri;
                let position = p.text_document_position.position;
                let snippets = self.snippets;
                let items = match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::completion(ws, &uri, position, snippets),
                    Some(DocKind::Tst) => tst::completion(ws, &uri, position, snippets),
                    _ => Vec::new(),
                };
                ok(lsp::CompletionResponse::Array(items))
            }
            req::InlayHintRequest::METHOD => {
                let p = params::<req::InlayHintRequest>(request)?;
                let uri = p.text_document.uri;
                ok(match kind(ws, &uri) {
                    Some(DocKind::Hdl) => hdl::inlay_hints(ws, &uri, p.range),
                    _ => Vec::new(),
                })
            }
            req::SemanticTokensFullRequest::METHOD => {
                let p = params::<req::SemanticTokensFullRequest>(request)?;
                ok(semantic_tokens::full(ws, &p.text_document.uri)
                    .map(lsp::SemanticTokensResult::Tokens))
            }
            other => Err((
                ErrorCode::MethodNotFound,
                format!("unhandled request `{other}`"),
            )),
        }
    }

    fn notification(&mut self, notification: Notification) -> Result<()> {
        match notification.method.as_str() {
            notif::DidOpenTextDocument::METHOD => {
                let p: lsp::DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = p.text_document.uri;
                self.workspace
                    .open(uri.clone(), p.text_document.text, p.text_document.version);
                self.refresh_around(&uri)?;
            }
            notif::DidChangeTextDocument::METHOD => {
                let p: lsp::DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = p.text_document.uri;
                if let Some(change) = p.content_changes.into_iter().last() {
                    self.workspace
                        .change(&uri, change.text, p.text_document.version);
                    self.refresh_around(&uri)?;
                }
            }
            notif::DidSaveTextDocument::METHOD => {
                let p: lsp::DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                self.refresh_around(&p.text_document.uri)?;
            }
            notif::DidCloseTextDocument::METHOD => {
                let p: lsp::DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let uri = p.text_document.uri;
                let dir = self.workspace.docs.get(&uri).map(|d| d.dir.clone());
                self.workspace.close(&uri);
                self.publish(&uri, Vec::new(), None)?;
                if let Some(dir) = dir {
                    self.refresh_dir(&dir)?;
                }
            }
            notif::DidChangeWatchedFiles::METHOD => {
                let dirs: Vec<_> = self.workspace.projects.keys().cloned().collect();
                for dir in dirs {
                    self.refresh_dir(&dir)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Re-publishes diagnostics for every open document in the same folder
    /// as `uri` (an edit to one chip can affect its users and scripts).
    fn refresh_around(&mut self, uri: &Url) -> Result<()> {
        let Some(dir) = self.workspace.docs.get(uri).map(|d| d.dir.clone()) else {
            return Ok(());
        };
        self.refresh_dir(&dir)
    }

    fn refresh_dir(&mut self, dir: &std::path::Path) -> Result<()> {
        self.workspace.invalidate_scripts(dir);
        for uri in self.workspace.docs_in(dir) {
            let diagnostics = self.workspace.diagnostics(&uri);
            let version = self.workspace.docs.get(&uri).map(|d| d.version);
            self.publish(&uri, diagnostics, version)?;
        }
        Ok(())
    }

    fn publish(
        &self,
        uri: &Url,
        diagnostics: Vec<lsp::Diagnostic>,
        version: Option<i32>,
    ) -> Result<()> {
        let params = lsp::PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version,
        };
        self.send(Message::Notification(Notification::new(
            notif::PublishDiagnostics::METHOD.to_string(),
            params,
        )))
    }
}
