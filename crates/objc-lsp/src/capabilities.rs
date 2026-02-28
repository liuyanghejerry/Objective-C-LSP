//! LSP capability advertisement.

use lsp_types::{
    CallHierarchyServerCapability, CodeActionOptions, CodeActionProviderCapability,
    CompletionOptions, DocumentSymbolOptions, FoldingRangeProviderCapability,
    HoverProviderCapability, ImplementationProviderCapability, InlayHintOptions,
    InlayHintServerCapabilities, OneOf, RenameOptions, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, WorkDoneProgressOptions,
};
use objc_syntax::tokens::semantic_tokens_options;

/// Build the capability set we advertise to the client during `initialize`.
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        // Sync: full document text on every change (incremental coming later).
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(lsp_types::TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),

        // Hover support.
        hover_provider: Some(HoverProviderCapability::Simple(true)),

        // Completions (trigger on `.`, `[`, `:`, `<`).
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![
                ".".into(),
                "[".into(),
                ":".into(),
                "<".into(),
                "#".into(),
            ]),
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            ..Default::default()
        }),

        // Document symbols (outline).
        document_symbol_provider: Some(OneOf::Right(DocumentSymbolOptions {
            label: Some("objc-lsp".into()),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),

        // Definition / declaration.
        definition_provider: Some(OneOf::Left(true)),
        declaration_provider: Some(lsp_types::DeclarationCapability::Simple(true)),

        // References.
        references_provider: Some(OneOf::Left(true)),

        // Semantic tokens (syntax highlighting).
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            semantic_tokens_options(),
        )),

        // Go-to-implementation (protocol implementors / method definitions).
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),

        // Inlay hints (message-send parameter labels).
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            },
        ))),

        // Rename (coordinated property rename).
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),

        // Code actions (protocol stub generation).
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![lsp_types::CodeActionKind::QUICKFIX]),
            resolve_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),

        // Workspace symbol search.
        workspace_symbol_provider: Some(OneOf::Left(true)),

        // Document formatting (clang-format).
        document_formatting_provider: Some(OneOf::Left(true)),

        // Folding ranges (tree-sitter).
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),

        // Call hierarchy (libclang).
        call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),

        // Type hierarchy (libclang) — handlers registered, capability advertised
        // via experimental until lsp-types adds type_hierarchy_provider to ServerCapabilities.

        // More capabilities will be added as features are implemented.
        ..Default::default()
    }
}
