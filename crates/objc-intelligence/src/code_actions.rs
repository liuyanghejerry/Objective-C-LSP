//! Tree-sitter–based code actions that do not require libclang.
//!
//! Actions provided:
//!
//! 1. **Generate `@interface`/`@implementation` pair** — if the cursor file
//!    is a `.m` file containing a `@implementation Foo` block with no
//!    corresponding `@interface` declaration in the *same file*, offer to
//!    create a stub `@interface Foo : NSObject … @end` at the top.
//!
//! 2. **Add `NS_ASSUME_NONNULL_BEGIN/END`** — if the file is an ObjC header
//!    and does not already contain `NS_ASSUME_NONNULL_BEGIN`, offer to wrap
//!    the entire file with the macros.

use anyhow::Result;
use lsp_types::{CodeAction, CodeActionKind, Position, Range, TextEdit, Uri, WorkspaceEdit};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Code-action parameters passed from the server.
pub struct CodeActionContext<'a> {
    pub uri: &'a Uri,
    /// Raw source text of the document.
    pub source: &'a str,
    /// File extension of the document (`h`, `m`, `mm`, …).
    pub extension: &'a str,
}

/// Compute tree-sitter–based code actions for the given document.
pub fn syntax_code_actions(ctx: &CodeActionContext<'_>) -> Result<Vec<CodeAction>> {
    let mut actions = Vec::new();

    if ctx.extension == "m" || ctx.extension == "mm" {
        if let Some(action) = generate_interface_action(ctx) {
            actions.push(action);
        }
    }

    if ctx.extension == "h" {
        if let Some(action) = add_nonnull_action(ctx) {
            actions.push(action);
        }
    }

    Ok(actions)
}

// ---------------------------------------------------------------------------
// Action 1 — Generate @interface stub
// ---------------------------------------------------------------------------

/// If the file has `@implementation ClassName` but no `@interface ClassName`,
/// offer to insert a minimal `@interface ClassName : NSObject … @end` above
/// the `@implementation`.
fn generate_interface_action(ctx: &CodeActionContext<'_>) -> Option<CodeAction> {
    // Collect all implementation class names (simple scan, no tree-sitter dep
    // needed for this heuristic).
    let impl_classes: Vec<&str> = ctx
        .source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("@implementation ") {
                // Stop at whitespace or end to get class name only.
                let class_name = rest.split_whitespace().next()?;
                // Skip categories: @implementation Foo (Cat)
                if rest.contains('(') {
                    return None;
                }
                Some(class_name)
            } else {
                None
            }
        })
        .collect();

    if impl_classes.is_empty() {
        return None;
    }

    // Check which class names already have @interface in this file.
    let has_interface = |name: &str| {
        ctx.source.lines().any(|line| {
            let t = line.trim();
            t.starts_with("@interface ") && t.contains(name)
        })
    };

    let missing: Vec<&str> = impl_classes
        .into_iter()
        .filter(|c| !has_interface(c))
        .collect();

    if missing.is_empty() {
        return None;
    }

    // Build the stub text to insert at line 0, character 0.
    let mut stub = String::new();
    for class_name in &missing {
        stub.push_str(&format!("@interface {class_name} : NSObject\n\n@end\n\n"));
    }

    let insert_position = Position {
        line: 0,
        character: 0,
    };
    let text_edit = TextEdit {
        range: Range {
            start: insert_position,
            end: insert_position,
        },
        new_text: stub,
    };

    let mut changes = HashMap::new();
    changes.insert(ctx.uri.clone(), vec![text_edit]);

    Some(CodeAction {
        title: format!("Generate @interface stub for {}", missing.join(", ")),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

// ---------------------------------------------------------------------------
// Action 2 — Add NS_ASSUME_NONNULL_BEGIN/END
// ---------------------------------------------------------------------------

/// If the header does not already contain `NS_ASSUME_NONNULL_BEGIN`, offer to
/// wrap all content between `NS_ASSUME_NONNULL_BEGIN` / `NS_ASSUME_NONNULL_END`.
fn add_nonnull_action(ctx: &CodeActionContext<'_>) -> Option<CodeAction> {
    if ctx.source.contains("NS_ASSUME_NONNULL_BEGIN") {
        return None;
    }

    // We want to insert:
    //   - `\nNS_ASSUME_NONNULL_BEGIN\n` after the last `#import` / `#include`
    //     line (or at start if there are none).
    //   - `\nNS_ASSUME_NONNULL_END\n` at the end of the file.

    let lines: Vec<&str> = ctx.source.lines().collect();
    let total_lines = lines.len() as u32;

    // Find last import/include line.
    let insert_after_line = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, l)| {
            let t = l.trim();
            t.starts_with("#import") || t.starts_with("#include")
        })
        .map(|(i, _)| i as u32)
        .unwrap_or(0);

    // Insert NS_ASSUME_NONNULL_BEGIN after last import.
    let begin_position = Position {
        line: insert_after_line + 1,
        character: 0,
    };
    let begin_edit = TextEdit {
        range: Range {
            start: begin_position,
            end: begin_position,
        },
        new_text: "\nNS_ASSUME_NONNULL_BEGIN\n".to_owned(),
    };

    // Append NS_ASSUME_NONNULL_END at the very end.
    let end_line = total_lines;
    let end_position = Position {
        line: end_line,
        character: 0,
    };
    let end_edit = TextEdit {
        range: Range {
            start: end_position,
            end: end_position,
        },
        new_text: "\nNS_ASSUME_NONNULL_END\n".to_owned(),
    };

    let mut changes = HashMap::new();
    changes.insert(ctx.uri.clone(), vec![begin_edit, end_edit]);

    Some(CodeAction {
        title: "Add NS_ASSUME_NONNULL_BEGIN/END".to_owned(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Uri;

    fn uri() -> Uri {
        "file:///tmp/Foo.h".parse().unwrap()
    }

    fn m_uri() -> Uri {
        "file:///tmp/Foo.m".parse().unwrap()
    }

    // --- generate_interface_action ---

    #[test]
    fn no_action_when_interface_present() {
        let src = "@interface Foo : NSObject\n@end\n@implementation Foo\n@end\n";
        let ctx = CodeActionContext {
            uri: &m_uri(),
            source: src,
            extension: "m",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions.iter().all(|a| !a.title.contains("@interface")),
            "should not offer stub when @interface already exists"
        );
    }

    #[test]
    fn action_offered_when_interface_missing() {
        let src = "@implementation Bar\n- (void)run {}\n@end\n";
        let ctx = CodeActionContext {
            uri: &m_uri(),
            source: src,
            extension: "m",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions.iter().any(|a| a.title.contains("Bar")),
            "expected @interface stub action for Bar, got: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_stub_action_for_category_impl() {
        let src = "@implementation Foo (Utils)\n- (void)help {}\n@end\n";
        let ctx = CodeActionContext {
            uri: &m_uri(),
            source: src,
            extension: "m",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions.iter().all(|a| !a.title.contains("@interface")),
            "should not offer stub for category @implementation"
        );
    }

    // --- add_nonnull_action ---

    #[test]
    fn nonnull_action_offered_for_header_without_it() {
        let src = "#import <Foundation/Foundation.h>\n\n@interface Baz : NSObject\n@end\n";
        let ctx = CodeActionContext {
            uri: &uri(),
            source: src,
            extension: "h",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| a.title.contains("NS_ASSUME_NONNULL")),
            "expected NS_ASSUME_NONNULL action, got: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nonnull_action_not_offered_when_already_present() {
        let src = "#import <Foundation/Foundation.h>\nNS_ASSUME_NONNULL_BEGIN\n@interface Baz : NSObject\n@end\nNS_ASSUME_NONNULL_END\n";
        let ctx = CodeActionContext {
            uri: &uri(),
            source: src,
            extension: "h",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions
                .iter()
                .all(|a| !a.title.contains("NS_ASSUME_NONNULL")),
            "should not offer nonnull action when already present"
        );
    }

    #[test]
    fn no_nonnull_action_for_m_files() {
        let src = "@implementation Foo\n@end\n";
        let ctx = CodeActionContext {
            uri: &m_uri(),
            source: src,
            extension: "m",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        assert!(
            actions
                .iter()
                .all(|a| !a.title.contains("NS_ASSUME_NONNULL")),
            "NS_ASSUME_NONNULL action should only be offered for headers"
        );
    }

    #[test]
    fn generated_interface_text_is_correct() {
        let src = "@implementation Qux\n- (void)go {}\n@end\n";
        let ctx = CodeActionContext {
            uri: &m_uri(),
            source: src,
            extension: "m",
        };
        let actions = syntax_code_actions(&ctx).unwrap();
        let action = actions.iter().find(|a| a.title.contains("Qux")).unwrap();
        let edit_text = action
            .edit
            .as_ref()
            .unwrap()
            .changes
            .as_ref()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .first()
            .unwrap()
            .new_text
            .as_str();
        assert!(
            edit_text.contains("@interface Qux : NSObject"),
            "stub should contain @interface Qux : NSObject, got: {edit_text}"
        );
        assert!(
            edit_text.contains("@end"),
            "stub should contain @end, got: {edit_text}"
        );
    }
}
