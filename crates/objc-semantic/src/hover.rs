//! Hover information from libclang cursors.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Return hover information for the given position in a file.
    pub fn hover_at(&self, path: &Path, pos: Position) -> Result<Option<Hover>> {
        // Extract TU pointer without holding the lock during clang calls.
        // (longjmp past a held Mutex would deadlock.)
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
            // Clang positions are 1-based.
            let path_cstr = path_to_cstr(path);
            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            if file.is_null() {
                // TU doesn't know this file (path mismatch) — nothing to hover.
                return Ok(None);
            }
            let location = unsafe { clang_getLocation(tu, file, pos.line + 1, pos.character + 1) };
            let cursor = unsafe { clang_getCursor(tu, location) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(None);
            }

            // Resolve to the canonical declaration cursor so we get full type
            // information (e.g. when hovering a reference, jump to the decl).
            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };
            let resolved = if unsafe { clang_Cursor_isNull(decl_cursor) } == 0 {
                decl_cursor
            } else {
                cursor
            };

            // Skip preprocessor / invalid cursors — calling type-spelling or
            // comment-text APIs on macro expansions with undefined macros (from
            // missing headers) causes a SIGSEGV inside libclang.
            let kind = unsafe { clang_getCursorKind(resolved) };
            if kind == CXCursor_InvalidCode || kind == CXCursor_NoDeclFound {
                return Ok(None);
            }
            // Preprocessor cursors: MacroExpansion=103, MacroDefinition=102, InclusionDirective=104
            // MacroExpansion cursors for undefined macros can SIGSEGV inside clang_getCursorDisplayName.
            let is_preprocessor = kind >= 100 && kind <= 110;
            if is_preprocessor {
                return Ok(None);
            }

            // Build a markdown string.
            let mut parts: Vec<String> = Vec::new();

            // --- Header line: kind + name ---
            let kind_str = cursor_kind_label(kind);
            let display = cx_string_owned(unsafe { clang_getCursorDisplayName(resolved) });

            // For ObjC interfaces/protocols: build a rich declaration signature.
            // e.g.  @interface UIViewController : UIResponder <NSCoding, UIAppearanceContainer>
            if kind == CXCursor_ObjCInterfaceDecl {
                let sig = build_interface_signature(resolved);
                if !sig.is_empty() {
                    parts.push(format!("```objc\n{sig}\n```"));
                } else if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
            } else if kind == CXCursor_ObjCProtocolDecl {
                let sig = build_protocol_signature(resolved);
                if !sig.is_empty() {
                    parts.push(format!("```objc\n{sig}\n```"));
                } else if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
            } else if kind == CXCursor_ObjCCategoryDecl {
                let sig = build_category_signature(resolved);
                if !sig.is_empty() {
                    parts.push(format!("```objc\n{sig}\n```"));
                } else if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
            } else if kind == CXCursor_ObjCInstanceMethodDecl
                || kind == CXCursor_ObjCClassMethodDecl
            {
                let sig = build_method_signature(resolved, kind);
                if !sig.is_empty() {
                    parts.push(format!("```objc\n{sig}\n```"));
                } else if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
            } else if kind == CXCursor_ObjCPropertyDecl {
                let sig = build_property_signature(resolved);
                if !sig.is_empty() {
                    parts.push(format!("```objc\n{sig}\n```"));
                } else if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
            } else {
                // Type spelling for non-ObjC symbols.
                if !display.is_empty() {
                    parts.push(format!("**{kind_str}** `{display}`"));
                }
                let ty = unsafe { clang_getCursorType(resolved) };
                if ty.kind != CXType_Invalid {
                    let ty_str = cx_string_owned(unsafe { clang_getTypeSpelling(ty) });
                    if !ty_str.is_empty() && ty_str != "void" && ty_str != display {
                        parts.push(format!("*Type:* `{ty_str}`"));
                    }
                }
            }

            // --- Doc comment ---
            // Try clang's built-in brief comment first, then fall back to
            // the raw comment text (handles Apple HeaderDoc `/*!` blocks).
            // If both are empty (common with -fmodules + PCM), attempt to
            // read the doc comment directly from the physical SDK header file.
            let doc = extract_doc_comment(resolved);
            if !doc.is_empty() {
                parts.push(doc);
            }

            if parts.is_empty() {
                return Ok(None);
            }

            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: parts.join("\n\n"),
                }),
                range: None,
            }))
        })
    }
}

// ---------------------------------------------------------------------------
// ObjC rich signature builders
// ---------------------------------------------------------------------------

/// Build `@interface ClassName : SuperClass <Proto1, Proto2>`
fn build_interface_signature(cursor: CXCursor) -> String {
    let name = cx_string_owned(unsafe { clang_getCursorSpelling(cursor) });
    if name.is_empty() {
        return String::new();
    }

    let super_cursor = unsafe { clang_getCursorDefinition(cursor) };
    // Walk children to find superclass ref and protocol refs.
    let mut super_name = String::new();
    let mut protocols: Vec<String> = Vec::new();

    visit_children(cursor, |child| {
        let child_kind = unsafe { clang_getCursorKind(child) };
        if child_kind == CXCursor_ObjCSuperClassRef {
            super_name = cx_string_owned(unsafe { clang_getCursorSpelling(child) });
        } else if child_kind == CXCursor_ObjCProtocolRef {
            let p = cx_string_owned(unsafe { clang_getCursorSpelling(child) });
            if !p.is_empty() {
                protocols.push(p);
            }
        }
        // Also check the definition cursor for superclass (needed when
        // hovering a reference rather than the @interface itself).
        _ = super_cursor;
    });

    let mut sig = format!("@interface {name}");
    if !super_name.is_empty() {
        sig.push_str(&format!(" : {super_name}"));
    }
    if !protocols.is_empty() {
        sig.push_str(&format!(" <{}>", protocols.join(", ")));
    }
    sig
}

/// Build `@protocol ProtoName <Parent1, Parent2>`
fn build_protocol_signature(cursor: CXCursor) -> String {
    let name = cx_string_owned(unsafe { clang_getCursorSpelling(cursor) });
    if name.is_empty() {
        return String::new();
    }

    let mut parents: Vec<String> = Vec::new();
    visit_children(cursor, |child| {
        let child_kind = unsafe { clang_getCursorKind(child) };
        if child_kind == CXCursor_ObjCProtocolRef {
            let p = cx_string_owned(unsafe { clang_getCursorSpelling(child) });
            if !p.is_empty() {
                parents.push(p);
            }
        }
    });

    let mut sig = format!("@protocol {name}");
    if !parents.is_empty() {
        sig.push_str(&format!(" <{}>", parents.join(", ")));
    }
    sig
}

/// Build `@interface ClassName (CategoryName)`
fn build_category_signature(cursor: CXCursor) -> String {
    let display = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
    if display.is_empty() {
        return String::new();
    }
    // display is already like "ClassName(CategoryName)"
    format!("@interface {display}")
}

/// Build `- (ReturnType)methodName:(ParamType)paramName ...`
fn build_method_signature(cursor: CXCursor, kind: CXCursorKind) -> String {
    let prefix = if kind == CXCursor_ObjCClassMethodDecl {
        "+"
    } else {
        "-"
    };

    let ret_type = {
        let ty = unsafe { clang_getCursorResultType(cursor) };
        if ty.kind == CXType_Invalid {
            "id".to_owned()
        } else {
            cx_string_owned(unsafe { clang_getTypeSpelling(ty) })
        }
    };

    // Build selector + params by walking child parameter cursors.
    // The display name already contains the full selector (e.g.
    // "initWithNibName:bundle:"), so we split it and pair with param types.
    let display = cx_string_owned(unsafe { clang_getCursorDisplayName(cursor) });
    if display.is_empty() {
        return String::new();
    }

    // Collect parameter cursors in order.
    let mut params: Vec<(String, String)> = Vec::new(); // (type, name)
    visit_children(cursor, |child| {
        if unsafe { clang_getCursorKind(child) } == CXCursor_ParmDecl {
            let ptype = {
                let ty = unsafe { clang_getCursorType(child) };
                cx_string_owned(unsafe { clang_getTypeSpelling(ty) })
            };
            let pname = cx_string_owned(unsafe { clang_getCursorSpelling(child) });
            params.push((ptype, pname));
        }
    });

    // Split selector into keyword parts.
    let keywords: Vec<&str> = display.split(':').collect();
    // keywords has one more element than colons (the trailing empty string
    // for multi-keyword selectors, or just the bare name for unary).

    if params.is_empty() {
        // Unary selector: `- (Type)name`
        return format!("{prefix} ({ret_type}){display}");
    }

    let mut sig = format!("{prefix} ({ret_type})");
    for (i, (ptype, pname)) in params.iter().enumerate() {
        let keyword = keywords.get(i).copied().unwrap_or("_");
        if i > 0 {
            sig.push(' ');
        }
        sig.push_str(&format!("{keyword}:({ptype}){pname}"));
    }
    sig
}

/// Build `@property (attrs) Type name`
fn build_property_signature(cursor: CXCursor) -> String {
    let name = cx_string_owned(unsafe { clang_getCursorSpelling(cursor) });
    if name.is_empty() {
        return String::new();
    }
    let ty = unsafe { clang_getCursorType(cursor) };
    let ty_str = if ty.kind != CXType_Invalid {
        cx_string_owned(unsafe { clang_getTypeSpelling(ty) })
    } else {
        "id".to_owned()
    };

    // Collect property attributes via ObjC property attribute flags.
    let attrs = unsafe { clang_Cursor_getObjCPropertyAttributes(cursor, 0) };
    let mut attr_parts: Vec<&str> = Vec::new();

    // CXObjCPropertyAttr_* flags from clang_sys / libclang header.
    // Values: nonatomic=0x20, readonly=0x8, readwrite=0x10,
    //         copy=0x4, retain/strong=0x200, weak=0x400, assign=0x2,
    //         getter=0x40 (custom getter — skip), setter=0x80 (skip)
    if attrs & 0x20 != 0 {
        attr_parts.push("nonatomic");
    } else {
        attr_parts.push("atomic");
    }
    if attrs & 0x8 != 0 {
        attr_parts.push("readonly");
    } else if attrs & 0x10 != 0 {
        attr_parts.push("readwrite");
    }
    if attrs & 0x200 != 0 {
        attr_parts.push("strong");
    } else if attrs & 0x400 != 0 {
        attr_parts.push("weak");
    } else if attrs & 0x4 != 0 {
        attr_parts.push("copy");
    } else if attrs & 0x2 != 0 {
        attr_parts.push("assign");
    }

    if attr_parts.is_empty() {
        format!("@property {ty_str} {name}")
    } else {
        format!("@property ({}) {ty_str} {name}", attr_parts.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Doc comment extraction
// ---------------------------------------------------------------------------

/// Extract doc comment for a cursor.
///
/// Strategy:
/// 1. `clang_Cursor_getBriefCommentText` — fast path, works when clang has
///    parsed the declaration with comments enabled.
/// 2. `clang_Cursor_getRawCommentText` — covers HeaderDoc `/*!` blocks.
/// 3. Physical SDK header fallback — when the declaration lives in a Clang
///    module PCM (binary), the comment APIs return empty. We then locate the
///    physical `.h` file via `clang_getFileName` on the cursor's source
///    location, read it, and extract the comment block that precedes the
///    declaration.
fn extract_doc_comment(cursor: CXCursor) -> String {
    // 1. Brief comment (usually filled for clangd-style /// comments).
    let brief = cx_string_owned(unsafe { clang_Cursor_getBriefCommentText(cursor) });
    if !brief.is_empty() {
        return brief;
    }

    // 2. Raw comment — covers Apple HeaderDoc `/*!` blocks.
    let raw = cx_string_owned(unsafe { clang_Cursor_getRawCommentText(cursor) });
    let cleaned = clean_raw_comment(&raw);
    if !cleaned.is_empty() {
        return cleaned;
    }

    // 3. Physical header fallback for module-cached declarations.
    //    Get the file where the declaration lives.
    if let Some(doc) = extract_doc_from_physical_header(cursor) {
        return doc;
    }

    String::new()
}

/// When clang comment APIs return nothing (typical with -fmodules PCM), read
/// the physical `.h` file and extract the HeaderDoc/Doxygen comment block
/// that immediately precedes the declaration at the given line.
fn extract_doc_from_physical_header(cursor: CXCursor) -> Option<String> {
    // Get the source location of the cursor.
    let loc = unsafe { clang_getCursorLocation(cursor) };
    let mut file: CXFile = std::ptr::null_mut();
    let mut line: u32 = 0;
    unsafe {
        clang_getSpellingLocation(
            loc,
            &mut file,
            &mut line,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
    if file.is_null() || line == 0 {
        return None;
    }

    let filename = cx_string_owned(unsafe { clang_getFileName(file) });
    if filename.is_empty() {
        return None;
    }

    // Only attempt to read actual .h files (not PCM/binary module files).
    if !filename.ends_with(".h") && !filename.ends_with(".hpp") {
        return None;
    }

    // Read the file.
    let content = std::fs::read_to_string(&filename).ok()?;
    let file_lines: Vec<&str> = content.lines().collect();
    let decl_line = (line as usize).saturating_sub(1); // 0-based

    // Walk backwards from the declaration line to find the comment block.
    extract_preceding_comment(&file_lines, decl_line)
}

/// Scan backwards from `decl_line` (0-based) to find a `/*!`, `/**`, or `///`
/// comment block that immediately precedes the declaration.
/// Returns None if no comment is found (non-comment lines between comment and decl).
fn extract_preceding_comment(lines: &[&str], decl_line: usize) -> Option<String> {
    if decl_line == 0 {
        return None;
    }

    // If there is a blank line immediately above the declaration, the comment
    // is not considered attached (Apple convention: no gap between comment and decl).
    if lines[decl_line - 1].trim().is_empty() {
        return None;
    }

    let end = decl_line;

    let last_comment_line = lines[end - 1].trim();

    // Case 1: Block comment ending with `*/`
    if last_comment_line.ends_with("*/") {
        // Walk back to find the opening `/*!` or `/**`.
        let mut start = end - 1;
        while start > 0 {
            let t = lines[start].trim();
            if t.starts_with("/*!") || t.starts_with("/**") || t.starts_with("/*") {
                break;
            }
            if start == 0 {
                break;
            }
            start -= 1;
        }
        let block = lines[start..end].join("\n");
        let cleaned = clean_raw_comment(&block);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }

    // Case 2: Line comments (`///` or `//!`)
    if last_comment_line.starts_with("///") || last_comment_line.starts_with("//!") {
        let mut start = end - 1;
        while start > 0 {
            let t = lines[start - 1].trim();
            if t.starts_with("///") || t.starts_with("//!") {
                start -= 1;
            } else {
                break;
            }
        }
        let block = lines[start..end].join("\n");
        let cleaned = clean_raw_comment(&block);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// libclang child visitor helper
// ---------------------------------------------------------------------------

/// Walk direct children of `cursor`, calling `f` for each.
///
/// Uses a safe wrapper around `clang_visitChildren` with a closure stored
/// on the stack (no heap allocation required).
fn visit_children<F: FnMut(CXCursor)>(cursor: CXCursor, mut f: F) {
    // We pass a pointer to the closure as the client_data.
    // The visitor is a plain extern "C" fn that casts it back and calls it.
    extern "C" fn visitor<F: FnMut(CXCursor)>(
        child: CXCursor,
        _parent: CXCursor,
        data: CXClientData,
    ) -> CXChildVisitResult {
        let f = unsafe { &mut *(data as *mut F) };
        f(child);
        CXChildVisit_Continue
    }

    let data = &mut f as *mut F as CXClientData;
    unsafe {
        clang_visitChildren(cursor, visitor::<F>, data);
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn cursor_kind_label(kind: CXCursorKind) -> &'static str {
    match kind {
        CXCursor_ObjCInterfaceDecl => "@interface",
        CXCursor_ObjCImplementationDecl => "@implementation",
        CXCursor_ObjCProtocolDecl => "@protocol",
        CXCursor_ObjCCategoryDecl => "category",
        CXCursor_ObjCInstanceMethodDecl => "instance method",
        CXCursor_ObjCClassMethodDecl => "class method",
        CXCursor_ObjCPropertyDecl => "@property",
        CXCursor_ObjCIvarDecl => "ivar",
        CXCursor_FunctionDecl => "function",
        CXCursor_VarDecl => "variable",
        CXCursor_TypedefDecl => "typedef",
        CXCursor_StructDecl => "struct",
        CXCursor_EnumDecl => "enum",
        CXCursor_MacroDefinition => "macro",
        _ => "symbol",
    }
}

fn path_to_cstr(path: &Path) -> std::ffi::CString {
    std::ffi::CString::new(path.to_string_lossy().as_ref())
        .expect("path must not contain null bytes")
}

fn cx_string_owned(s: CXString) -> String {
    let ptr = unsafe { clang_getCString(s) };
    let result = if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
    };
    unsafe { clang_disposeString(s) };
    result
}

/// Strip comment delimiters from a raw Clang comment string and return
/// clean prose suitable for Markdown hover rendering.
///
/// Handles all common Apple/Doxygen doc comment styles:
/// - `/*!` … `*/`  (Apple HeaderDoc)
/// - `/**` … `*/`  (Doxygen block)
/// - `///` / `//!` line comments
fn clean_raw_comment(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = raw.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        let t = line.trim();
        // Strip opening markers.
        let t = t
            .strip_prefix("/*!")
            .or_else(|| t.strip_prefix("/**"))
            .or_else(|| t.strip_prefix("//!"))
            .or_else(|| t.strip_prefix("///"))
            .unwrap_or(t);
        // Strip closing marker.
        let t = t.trim_end_matches("*/").trim();
        // Strip leading ` * ` from block comment body lines.
        let t = t
            .strip_prefix("* ")
            .unwrap_or(t.strip_prefix('*').unwrap_or(t));
        // Convert @param / @return tags to Markdown bold.
        let line_out = convert_doc_tag(t.trim());
        if !line_out.is_empty() {
            out.push(line_out);
        }
    }

    out.join("\n")
}

/// Convert `@param foo desc` / `@return desc` to Markdown-friendly text.
fn convert_doc_tag(line: &str) -> String {
    if let Some(rest) = line
        .strip_prefix("@param ")
        .or_else(|| line.strip_prefix("\\param "))
    {
        let mut parts = rest.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let desc = parts.next().unwrap_or("").trim();
        if desc.is_empty() {
            return format!("**Parameter** `{name}`");
        }
        return format!("**Parameter** `{name}`: {desc}");
    }
    if let Some(rest) = line
        .strip_prefix("@return ")
        .or_else(|| line.strip_prefix("\\return "))
    {
        return format!("**Returns**: {}", rest.trim());
    }
    if let Some(rest) = line.strip_prefix("@abstract ") {
        return rest.trim().to_owned();
    }
    if let Some(rest) = line.strip_prefix("@discussion ") {
        return rest.trim().to_owned();
    }
    line.to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_apple_headerdoc_block() {
        let raw = "/*!\n * @abstract Returns the name.\n * @param index The index.\n * @return The name string.\n */";
        let result = clean_raw_comment(raw);
        assert!(result.contains("Returns the name"), "got: {result}");
        assert!(result.contains("Parameter"), "got: {result}");
        assert!(result.contains("**Returns**"), "got: {result}");
    }

    #[test]
    fn cleans_triple_slash_comments() {
        let raw = "/// First line.\n/// Second line.";
        let result = clean_raw_comment(raw);
        assert!(result.contains("First line"), "got: {result}");
        assert!(result.contains("Second line"), "got: {result}");
    }

    #[test]
    fn empty_raw_returns_empty() {
        assert_eq!(clean_raw_comment(""), "");
    }

    #[test]
    fn converts_param_tag() {
        let result = convert_doc_tag("@param name The name of the object.");
        assert!(result.contains("**Parameter**"), "got: {result}");
        assert!(result.contains("`name`"), "got: {result}");
    }

    #[test]
    fn converts_return_tag() {
        let result = convert_doc_tag("@return The resulting value.");
        assert!(result.contains("**Returns**"), "got: {result}");
    }

    #[test]
    fn extract_preceding_comment_block() {
        let lines = vec![
            "/*!",
            " * @abstract A view controller.",
            " */",
            "@interface UIViewController : UIResponder",
        ];
        let result = extract_preceding_comment(&lines, 3);
        assert!(result.is_some(), "should find block comment");
        let s = result.unwrap();
        assert!(s.contains("A view controller"), "got: {s}");
    }

    #[test]
    fn extract_preceding_comment_line_style() {
        let lines = vec![
            "/// Presents content.",
            "/// Use this method to show content.",
            "- (void)presentViewController:(UIViewController *)vc;",
        ];
        let result = extract_preceding_comment(&lines, 2);
        assert!(result.is_some(), "should find line comments");
        let s = result.unwrap();
        assert!(s.contains("Presents content"), "got: {s}");
    }

    #[test]
    fn extract_preceding_comment_none_when_blank_gap() {
        let lines = vec!["/// Some comment.", "", "- (void)foo;"];
        // blank line between comment and decl: no comment attached
        let result = extract_preceding_comment(&lines, 2);
        assert!(
            result.is_none(),
            "blank line should break comment attachment"
        );
    }
}
