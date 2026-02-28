//! Main server loop: owns all state and processes LSP messages.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{
    CodeActionRequest, Completion, DocumentSymbolRequest, GotoDeclaration, GotoDefinition,
    GotoImplementation, HoverRequest, InlayHintRequest, References, Rename,
    Request as _, SemanticTokensFullRequest,
};
use lsp_types::{
    CodeActionParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, HoverParams, InitializeParams,
    InitializeResult, InlayHintParams, PublishDiagnosticsParams, ReferenceParams,
    RenameParams, SemanticTokensParams, ServerInfo, Uri,
};

use objc_project::{compile_db::CompileCommandsDb, sdk, FlagResolver};
use objc_semantic::ClangIndex;
use objc_syntax::{inlay_hints::inlay_hints, symbols::document_symbols, tokens::semantic_tokens_full, ObjcParser};

use crate::capabilities::server_capabilities;

/// Per-document state.
struct Document {
    _uri: Uri,
    content: String,
}

/// All server state.
pub struct Server {
    connection: Connection,
    documents: HashMap<Uri, Document>,
    parser: Arc<Mutex<ObjcParser>>,
    /// libclang index for type-aware features.
    clang_index: Arc<ClangIndex>,
    /// Resolves per-file compile flags (from compile_commands.json or xcodeproj).
    flag_resolver: Option<Arc<dyn FlagResolver>>,
    /// Default SDK / GNUstep include flags (always prepended).
    base_flags: Vec<String>,
}

impl Server {
    fn new(connection: Connection, workspace_root: Option<PathBuf>) -> Result<Self> {
        // Best-effort: load compile_commands.json from the workspace root.
        let flag_resolver: Option<Arc<dyn FlagResolver>> = workspace_root
            .as_deref()
            .and_then(CompileCommandsDb::find_and_load)
            .map(|db| Arc::new(db) as Arc<dyn FlagResolver>);

        let base_flags = sdk::default_include_flags();

        let clang_index = Arc::new(ClangIndex::new()?);

        Ok(Self {
            connection,
            documents: HashMap::new(),
            parser: Arc::new(Mutex::new(ObjcParser::new()?)),
            clang_index,
            flag_resolver,
            base_flags,
        })
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Resolve compilation flags for a file: compile_db flags + base SDK flags.
    fn flags_for(&self, path: &Path) -> Vec<String> {
        let mut flags = self.base_flags.clone();
        if let Some(resolver) = &self.flag_resolver {
            if let Some(cf) = resolver.flags_for(path) {
                flags.extend(cf.args);
            }
        }
        flags
    }

    /// URI → filesystem path (strips `file://` prefix).
    fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
        let s = uri.as_str();
        if let Some(p) = s.strip_prefix("file://") {
            Some(PathBuf::from(p))
        } else {
            None
        }
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
            SemanticTokensFullRequest::METHOD => self.on_semantic_tokens_full(req)?,
            References::METHOD => self.on_references(req)?,
            GotoImplementation::METHOD => self.on_goto_implementation(req)?,
            InlayHintRequest::METHOD => self.on_inlay_hint(req)?,
            Rename::METHOD => self.on_rename(req)?,
            CodeActionRequest::METHOD => self.on_code_action(req)?,
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
                _uri: uri.clone(),
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
        let uri = &params.text_document.uri;
        self.documents.remove(uri);
        if let Some(path) = Self::uri_to_path(uri) {
            self.clang_index.dispose_file(&path);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Requests
    // -----------------------------------------------------------------------

    fn on_document_symbol(&mut self, req: Request) -> Result<()> {
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
        let (id, params): (RequestId, HoverParams) = req.extract(HoverRequest::METHOD)?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.hover_at(&path, pos) {
                Ok(Some(hover)) => serde_json::to_value(hover)?,
                Ok(None) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("hover error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_completion(&mut self, req: Request) -> Result<()> {
        use lsp_types::{CompletionParams, CompletionResponse};

        let (id, params): (RequestId, CompletionParams) = req.extract(Completion::METHOD)?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let items = if let Some(path) = Self::uri_to_path(uri) {
            // Pass the current (possibly unsaved) buffer content so libclang
            // can complete without reading stale files from disk.
            let content = self.documents.get(uri).map(|d| d.content.as_str());
            match self.clang_index.completions_at(&path, pos, content) {
                Ok(items) => items,
                Err(e) => {
                    tracing::warn!("completion error: {e}");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let resp = Response::new_ok(
            id,
            serde_json::to_value(CompletionResponse::Array(items))?,
        );
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_goto_definition(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, GotoDefinitionParams) =
            req.extract(GotoDefinition::METHOD)?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.definition_at(&path, pos) {
                Ok(Some(loc)) => serde_json::to_value(loc)?,
                Ok(None) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("goto-definition error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_goto_declaration(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, GotoDefinitionParams) =
            req.extract(GotoDeclaration::METHOD)?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.declaration_at(&path, pos) {
                Ok(Some(loc)) => serde_json::to_value(loc)?,
                Ok(None) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("goto-declaration error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_references(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, ReferenceParams) = req.extract(References::METHOD)?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_decl = params.context.include_declaration;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.references_at(&path, pos, include_decl) {
                Ok(locs) if !locs.is_empty() => serde_json::to_value(locs)?,
                Ok(_) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("find-references error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_semantic_tokens_full(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, SemanticTokensParams) =
            req.extract(SemanticTokensFullRequest::METHOD)?;
        let uri = &params.text_document.uri;

        let result = if let Some(doc) = self.documents.get(uri) {
            let mut parser = self.parser.lock().unwrap();
            match parser.parse(&doc.content) {
                Ok(parsed) => match semantic_tokens_full(&parsed) {
                    Ok(toks) => serde_json::to_value(toks)?,
                    Err(e) => {
                        tracing::warn!("semantic tokens error: {e}");
                        serde_json::Value::Null
                    }
                },
                Err(e) => {
                    tracing::warn!("parse error for semantic tokens: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_goto_implementation(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, GotoDefinitionParams) =
            req.extract(GotoImplementation::METHOD)?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.implementations_of(&path, pos) {
                Ok(locs) if !locs.is_empty() => serde_json::to_value(locs)?,
                Ok(_) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("goto-implementation error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_inlay_hint(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, InlayHintParams) =
            req.extract(InlayHintRequest::METHOD)?;
        let uri = &params.text_document.uri;
        let range = params.range;

        let result = if let Some(doc) = self.documents.get(uri) {
            let mut parser = self.parser.lock().unwrap();
            match parser.parse(&doc.content) {
                Ok(parsed) => match inlay_hints(&parsed, Some(range)) {
                    Ok(hints) if !hints.is_empty() => serde_json::to_value(hints)?,
                    Ok(_) => serde_json::Value::Array(vec![]),
                    Err(e) => {
                        tracing::warn!("inlay hints error: {e}");
                        serde_json::Value::Array(vec![])
                    }
                },
                Err(e) => {
                    tracing::warn!("parse error for inlay hints: {e}");
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

    fn on_rename(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, RenameParams) = req.extract(Rename::METHOD)?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = &params.new_name;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.rename_at(&path, pos, new_name) {
                Ok(Some(edit)) => serde_json::to_value(edit)?,
                Ok(None) => serde_json::Value::Null,
                Err(e) => {
                    tracing::warn!("rename error: {e}");
                    serde_json::Value::Null
                }
            }
        } else {
            serde_json::Value::Null
        };

        let resp = Response::new_ok(id, result);
        self.connection.sender.send(Message::Response(resp))?;
        Ok(())
    }

    fn on_code_action(&mut self, req: Request) -> Result<()> {
        let (id, params): (RequestId, CodeActionParams) =
            req.extract(CodeActionRequest::METHOD)?;
        let uri = &params.text_document.uri;
        let range = params.range;

        let result = if let Some(path) = Self::uri_to_path(uri) {
            match self.clang_index.code_actions_at(&path, range, uri) {
                Ok(actions) if !actions.is_empty() => serde_json::to_value(actions)?,
                Ok(_) => serde_json::Value::Array(vec![]),
                Err(e) => {
                    tracing::warn!("code action error: {e}");
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

    // -----------------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------------

    fn publish_diagnostics(&self, uri: &Uri, _content: &str) -> Result<()> {
        let diagnostics = if let Some(path) = Self::uri_to_path(uri) {
            let flags = self.flags_for(&path);
            // Write content to a temp file so libclang can parse unsaved buffers.
            // Use unsaved_files mechanism via clang_parseTranslationUnit for production;
            // for now parse from disk path (file must exist) or skip silently.
            match self.clang_index.parse_file(&path, &flags) {
                Ok(()) => match self.clang_index.diagnostics_for(&path) {
                    Ok(diags) => diags,
                    Err(e) => {
                        tracing::warn!("diagnostics_for error: {e}");
                        vec![]
                    }
                },
                Err(e) => {
                    tracing::debug!("parse_file skipped (file may not be on disk): {e}");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
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

    // Derive workspace root from the first workspace folder or root_uri.
    #[allow(deprecated)]
    let workspace_root: Option<PathBuf> = init_params
        .workspace_folders
        .as_deref()
        .and_then(|f| f.first())
        .and_then(|wf| {
            let s = wf.uri.as_str();
            s.strip_prefix("file://").map(PathBuf::from)
        })
        .or_else(|| {
            init_params
                .root_uri
                .as_ref()
                .and_then(|u| u.as_str().strip_prefix("file://").map(PathBuf::from))
        });

    tracing::info!(
        client = init_params
            .client_info
            .as_ref()
            .map(|c| c.name.as_str())
            .unwrap_or("unknown"),
        workspace_root = ?workspace_root,
        "initialized"
    );

    let mut server = Server::new(connection, workspace_root)?;

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
