//! Main server loop: owns all state and processes LSP messages.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDeclaration, GotoDefinition, HoverRequest, Initialize,
    Request as _,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, PublishDiagnosticsParams, ServerInfo, Uri,
};

use objc_syntax::{symbols::document_symbols, ObjcParser};

use crate::capabilities::server_capabilities;

/// Per-document state.
struct Document {
    uri: Uri,
    content: String,
}

/// All server state.
pub struct Server {
    connection: Connection,
    documents: HashMap<Uri, Document>,
    parser: Arc<Mutex<ObjcParser>>,
}

impl Server {
    fn new(connection: Connection) -> Result<Self> {
        Ok(Self {
            connection,
            documents: HashMap::new(),
            parser: Arc::new(Mutex::new(ObjcParser::new()?)),
        })
    }

    // -----------------------------------------------------------------------
    // Message dispatch
    // -----------------------------------------------------------------------

    fn handle_request(&mut self, req: Request) -> Result<()> {
        match req.method.as_str() {
            DocumentSymbolRequest::METHOD => self.on_document_symbol(req)?,
            HoverRequest::METHOD => self.on_hover(req)?,
            Completion::METHOD => self.on_completion(req)?,
            GotoDefinition::METHOD => self.on_goto_definition(req)?,
            GotoDeclaration::METHOD => self.on_goto_declaration(req)?,
            _ => {
                // Unknown request: send empty response to avoid client timeout.
                let resp = Response::new_ok(req.id, serde_json::Value::Null);
                self.connection.sender.send(Message::Response(resp))?;
            }
        }
        Ok(())
    }

    fn handle_notification(&mut self, notif: Notification) -> Result<()> {
        match notif.method.as_str() {
            DidOpenTextDocument::METHOD => self.on_did_open(notif)?,
            DidChangeTextDocument::METHOD => self.on_did_change(notif)?,
            DidCloseTextDocument::METHOD => self.on_did_close(notif)?,
            _ => {}
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Document sync
    // -----------------------------------------------------------------------

    fn on_did_open(&mut self, notif: Notification) -> Result<()> {
        let params: DidOpenTextDocumentParams = serde_json::from_value(notif.params)?;
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();
        self.documents.insert(
            uri.clone(),
            Document {
                uri: uri.clone(),
                content: content.clone(),
            },
        );
        self.publish_diagnostics(&uri, &content)?;
        Ok(())
    }

    fn on_did_change(&mut self, notif: Notification) -> Result<()> {
        let params: DidChangeTextDocumentParams = serde_json::from_value(notif.params)?;
        let uri = params.text_document.uri.clone();
        // We requested FULL sync, so the last change contains the whole text.
        if let Some(change) = params.content_changes.into_iter().last() {
            let content = change.text.clone();
            if let Some(doc) = self.documents.get_mut(&uri) {
                doc.content = content.clone();
            }
            self.publish_diagnostics(&uri, &content)?;
        }
        Ok(())
    }

    fn on_did_close(&mut self, notif: Notification) -> Result<()> {
        let params: DidCloseTextDocumentParams = serde_json::from_value(notif.params)?;
        self.documents.remove(&params.text_document.uri);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Requests
    // -----------------------------------------------------------------------

    fn on_document_symbol(&mut self, req: Request) -> Result<()> {
        use lsp_types::request::DocumentSymbolRequest;
        use lsp_types::DocumentSymbolParams;

        let (id, params): (RequestId, DocumentSymbolParams) =
            req.extract(DocumentSymbolRequest::METHOD)?;

        let uri = &params.text_document.uri;
        let result = if let Some(doc) = self.documents.get(uri) {
            let mut parser = self.parser.lock().unwrap();
            match parser.parse(&doc.content) {
                Ok(parsed) => match document_symbols(&parsed) {
                    Ok(syms) => serde_json::to_value(syms)?,
                    Err(e) => {
                        tracing::warn!("documentSymbol error: {e}");
                        serde_json::Value::Array(vec![])
                    }
                },
                Err(e) => {
                    tracing::warn!("parse error: {e}");
                    serde_json::Value::Array(vec![])
                }
            }
        } else {
            serde_json::Value::Array(vec![])
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_hover(&mut self, req: Request) -> Result<()> {
        // Placeholder: semantic hover requires libclang integration.
        let resp = Response::new_ok(req.id, serde_json::Value::Null);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_completion(&mut self, req: Request) -> Result<()> {
        // Placeholder: completions require libclang integration.
        let resp = Response::new_ok(
            req.id,
            serde_json::json!({"isIncomplete": false, "items": []}),
        );
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_goto_definition(&mut self, req: Request) -> Result<()> {
        // Placeholder.
        let resp = Response::new_ok(req.id, serde_json::Value::Null);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_goto_declaration(&mut self, req: Request) -> Result<()> {
        // Placeholder: jump to @interface declaration.
        let resp = Response::new_ok(req.id, serde_json::Value::Null);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------------

    fn publish_diagnostics(&self, uri: &Uri, _content: &str) -> Result<()> {
        // For now, publish empty diagnostics (clang integration comes next).
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![],
            version: None,
        };
        let notif = Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
        self.connection.sender.send(Message::Notification(notif))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run(connection: Connection) -> Result<()> {
    // Perform the initialize handshake.
    let init_params: InitializeParams = {
        let (id, params) = connection.initialize_start()?;
        let result = InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
                name: "objc-lsp".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        };
        connection.initialize_finish(id, serde_json::to_value(result)?)?;
        serde_json::from_value(params)?
    };

    tracing::info!(
        client = init_params
            .client_info
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("unknown"),
        "initialized"
    );

    let mut server = Server::new(connection)?;

    // Main message loop.
    loop {
        match server.connection.receiver.recv()? {
            Message::Request(req) => {
                if server.connection.handle_shutdown(&req)? {
                    break;
                }
                if let Err(e) = server.handle_request(req) {
                    tracing::error!("request error: {e}");
                }
            }
            Message::Notification(notif) => {
                if let Err(e) = server.handle_notification(notif) {
                    tracing::error!("notification error: {e}");
                }
            }
            Message::Response(_) => {}
        }
    }

    Ok(())
}
