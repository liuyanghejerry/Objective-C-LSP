//! Go-to-definition and go-to-declaration via libclang cursors.

use std::ffi::CStr;
use std::path::Path;

use anyhow::Result;
use clang_sys::*;
use lsp_types::{GotoDefinitionResponse, Location, Position, Range, Uri};
use tracing::info;

use crate::crash_guard::with_crash_guard;
use crate::index::ClangIndex;

impl ClangIndex {
    /// Return the definition location for the symbol under the cursor.
    ///
    /// Strategy:
    ///
    /// 1. `clang_getCursorDefinition` — finds the `@implementation` body for ObjC
    ///    methods, or the struct/function body for C.
    ///
    /// 2. Fall back to `clang_getCursorReferenced` — ObjC classes/protocols declared
    ///    in SDK headers have no "definition" in the C++ sense; the reference cursor
    ///    resolves to their `@interface` declaration in the physical `.h` file.
    pub fn definition_at(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
            let path_cstr = path_to_cstr(path);

            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };

            if file.is_null() {
                return Ok(None);
            }

            let location = unsafe { clang_getLocation(tu, file, pos.line + 1, pos.character + 1) };

            let cursor = unsafe { clang_getCursor(tu, location) };
            let cursor_kind = unsafe { clang_getCursorKind(cursor) };
            info!(
                "definition_at: cursor kind={} pos={}:{}",
                cursor_kind as u32,
                pos.line + 1,
                pos.character + 1
            );

            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(None);
            }

            // 1. Try definition (e.g. @implementation body, function body).
            let def_cursor = unsafe { clang_getCursorDefinition(cursor) };
            if unsafe { clang_Cursor_isNull(def_cursor) } == 0 {
                if let Some(loc) = cursor_to_location(def_cursor)? {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
            }

            // 2. Fall back to canonical declaration (covers SDK @interface / @protocol
            //    types that have no @implementation in the current TU).
            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(decl_cursor) } == 0 {
                if let Some(loc) = cursor_to_location(decl_cursor)? {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
            }

            // 3. AST child-walk fallback: clang sometimes returns a container
            //    cursor (DeclStmt, ObjCMessageExpr, etc.) instead of the token-level
            //    cursor when the receiver is at certain column offsets. Walk the
            //    cursor's children to find the deepest descendant that contains the
            //    target position and has a resolvable referenced cursor.
            info!("definition_at: reaching strategy-3 fallback");
            if let Some(loc) = child_cursor_at_location(tu, cursor, location)? {
                return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
            }

            Ok(None)
        })
    }

    /// Return the declaration location for the symbol under the cursor.
    ///
    /// Uses `clang_getCursorReferenced` — for ObjC methods this jumps to the
    /// `@interface` / `@protocol` declaration rather than the `@implementation`.
    pub fn declaration_at(
        &self,
        path: &Path,
        pos: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tu = {
            let units = self.units.lock().unwrap();
            match units.get(path) {
                Some(tu) => *tu,
                None => return Ok(None),
            }
        };

        with_crash_guard(|| {
            let path_cstr = path_to_cstr(path);
            let file = unsafe { clang_getFile(tu, path_cstr.as_ptr()) };
            if file.is_null() {
                return Ok(None);
            }
            let location = unsafe { clang_getLocation(tu, file, pos.line + 1, pos.character + 1) };

            let cursor = unsafe { clang_getCursor(tu, location) };
            if unsafe { clang_Cursor_isNull(cursor) } != 0 {
                return Ok(None);
            }

            // `clang_getCursorReferenced` gives the canonical declaration.
            let decl_cursor = unsafe { clang_getCursorReferenced(cursor) };
            if unsafe { clang_Cursor_isNull(decl_cursor) } != 0 {
                return Ok(None);
            }

            cursor_to_location(decl_cursor).map(|loc| loc.map(GotoDefinitionResponse::Scalar))
        })
    }
}

/// Convert a libclang cursor to an LSP `Location` pointing at its spelling location.
///
/// Uses `clang_getCursorLocation` (the cursor's canonical source location) rather than
/// the extent range-start.  For SDK declarations resolved via module caches the spelling
/// location resolves to the physical `.h` file; the extent start may point at a PCM binary.
fn cursor_to_location(cursor: CXCursor) -> Result<Option<Location>> {
    // Prefer the cursor's own spelling location: for SDK symbols loaded via
    // -include (plain #import chain) this always resolves to the physical .h file.
    let loc = unsafe { clang_getCursorLocation(cursor) };

    let mut file: CXFile = std::ptr::null_mut();
    let mut line: u32 = 0;
    let mut col: u32 = 0;
    unsafe {
        clang_getSpellingLocation(loc, &mut file, &mut line, &mut col, std::ptr::null_mut());
    }

    // Fallback: if spelling location yields no file (e.g. a residual PCM reference),
    // try the expansion location which more reliably resolves to the physical header.
    if file.is_null() || line == 0 {
        unsafe {
            clang_getExpansionLocation(loc, &mut file, &mut line, &mut col, std::ptr::null_mut());
        }
    }

    if file.is_null() || line == 0 {
        return Ok(None);
    }

    // Get the file path from the CXFile.
    let cx_filename = unsafe { clang_getFileName(file) };
    let filename = cx_string_owned(cx_filename);

    if filename.is_empty() {
        return Ok(None);
    }

    // Build a file:// URI.
    let uri: Uri = format!("file://{filename}")
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid URI for {filename}: {e}"))?;

    // Derive the selection-range end from the cursor extent, but only when the
    // extent end lives in the same file (not a macro expansion or PCM offset).
    let extent = unsafe { clang_getCursorExtent(cursor) };
    let end_loc = unsafe { clang_getRangeEnd(extent) };
    let mut end_line = line;
    let mut end_col = col;
    let mut end_file: CXFile = std::ptr::null_mut();
    unsafe {
        clang_getSpellingLocation(
            end_loc,
            &mut end_file,
            &mut end_line,
            &mut end_col,
            std::ptr::null_mut(),
        );
    }

    if end_file != file {
        // Extent is in a different file (PCM, macro expansion) — use zero-width range.
        end_line = line;
        end_col = col;
    }

    let range = Range {
        start: lsp_types::Position {
            line: line.saturating_sub(1),
            character: col.saturating_sub(1),
        },
        end: lsp_types::Position {
            line: end_line.saturating_sub(1),
            character: end_col.saturating_sub(1),
        },
    };

    Ok(Some(Location { uri, range }))
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

/// Fallback goto-definition when `clang_getCursor` returns a coarse container.
///
/// libclang sometimes returns a `DeclStmt` for every column in a line when the
/// statement is a variable declaration with an ObjC message-send initializer.
/// In that case, `clang_visitChildren` only exposes the type-annotation child,
/// not the initializer expression.  This function uses two strategies:
///
/// 1. **AST child-walk**: `clang_visitChildren` from the container cursor, looking
///    for the deepest child whose extent contains the target position and whose
///    `clang_getCursorReferenced` is non-null.
///
/// 2. **Token-annotation same-spelling fallback**: Tokenize the container's source
///    range, find the token at the clicked position, then find another annotated
///    token in the same range with identical spelling and a non-null reference.
///    This resolves the case where the receiver class (`UIBezierPath`) in a
///    message send is invisible to the cursor API but its type-annotation twin
///    earlier in the same DeclStmt is fully resolved.
///
/// Returns `Some(location)` on success, `None` if neither strategy finds anything.
fn child_cursor_at_location(
    tu: CXTranslationUnit,
    container: CXCursor,
    location: CXSourceLocation,
) -> Result<Option<Location>> {
    // ── Strategy 1: AST child-walk ──────────────────────────────────────────
    struct VisitCtx {
        target: CXSourceLocation,
        best: Option<CXCursor>,
    }

    extern "C" fn visitor(
        cursor: CXCursor,
        _parent: CXCursor,
        data: CXClientData,
    ) -> CXChildVisitResult {
        let ctx = unsafe { &mut *(data as *mut VisitCtx) };

        let extent = unsafe { clang_getCursorExtent(cursor) };
        if unsafe { clang_Range_isNull(extent) } != 0 {
            return CXChildVisit_Continue;
        }

        let mut start_file: CXFile = std::ptr::null_mut();
        let mut start_line: u32 = 0;
        let mut start_col: u32 = 0;
        let mut end_line: u32 = 0;
        let mut end_col: u32 = 0;
        let mut end_file: CXFile = std::ptr::null_mut();
        let range_start = unsafe { clang_getRangeStart(extent) };
        let range_end = unsafe { clang_getRangeEnd(extent) };
        unsafe {
            clang_getSpellingLocation(
                range_start,
                &mut start_file,
                &mut start_line,
                &mut start_col,
                std::ptr::null_mut(),
            );
            clang_getSpellingLocation(
                range_end,
                &mut end_file,
                &mut end_line,
                &mut end_col,
                std::ptr::null_mut(),
            );
        }

        let mut tgt_file: CXFile = std::ptr::null_mut();
        let mut tgt_line: u32 = 0;
        let mut tgt_col: u32 = 0;
        unsafe {
            clang_getSpellingLocation(
                ctx.target,
                &mut tgt_file,
                &mut tgt_line,
                &mut tgt_col,
                std::ptr::null_mut(),
            );
        }

        if start_file.is_null() || tgt_file.is_null() || start_file != tgt_file {
            return CXChildVisit_Continue;
        }

        let after_start = (tgt_line, tgt_col) >= (start_line, start_col);
        let before_end = (tgt_line, tgt_col) <= (end_line, end_col);
        if !after_start || !before_end {
            return CXChildVisit_Continue;
        }

        let ref_cursor = unsafe { clang_getCursorReferenced(cursor) };
        if unsafe { clang_Cursor_isNull(ref_cursor) } == 0 {
            ctx.best = Some(cursor);
        }

        CXChildVisit_Recurse
    }

    let mut ctx = VisitCtx {
        target: location,
        best: None,
    };
    unsafe {
        clang_visitChildren(
            container,
            visitor,
            &mut ctx as *mut VisitCtx as CXClientData,
        );
    }

    if let Some(best_cursor) = ctx.best {
        let def_cursor = unsafe { clang_getCursorDefinition(best_cursor) };
        if unsafe { clang_Cursor_isNull(def_cursor) } == 0 {
            if let Some(loc) = cursor_to_location(def_cursor)? {
                return Ok(Some(loc));
            }
        }
        let decl_cursor = unsafe { clang_getCursorReferenced(best_cursor) };
        if unsafe { clang_Cursor_isNull(decl_cursor) } == 0 {
            if let Some(loc) = cursor_to_location(decl_cursor)? {
                return Ok(Some(loc));
            }
        }
    }

    // ── Strategy 2: Token-annotation same-spelling fallback ─────────────────
    //
    // For patterns like `UIBezierPath *v = [UIBezierPath msg]`, libclang maps the
    // receiver token to DeclStmt but the type-annotation token resolves correctly.
    // Tokenize the container extent, find the spelling of the clicked token, then
    // find any annotated token with that spelling whose referenced cursor is non-null.
    let container_extent = unsafe { clang_getCursorExtent(container) };
    if unsafe { clang_Range_isNull(container_extent) } != 0 {
        return Ok(None);
    }

    let mut tokens_ptr: *mut CXToken = std::ptr::null_mut();
    let mut token_count: u32 = 0;
    unsafe {
        clang_tokenize(tu, container_extent, &mut tokens_ptr, &mut token_count);
    }
    if token_count == 0 || tokens_ptr.is_null() {
        return Ok(None);
    }

    // Safety: defer disposal.
    struct TokenGuard(*mut CXToken, u32, CXTranslationUnit);
    impl Drop for TokenGuard {
        fn drop(&mut self) {
            unsafe { clang_disposeTokens(self.2, self.0, self.1) };
        }
    }
    let _guard = TokenGuard(tokens_ptr, token_count, tu);

    let token_slice = unsafe { std::slice::from_raw_parts(tokens_ptr, token_count as usize) };
    let mut ann_cursors: Vec<CXCursor> = vec![unsafe { std::mem::zeroed() }; token_count as usize];
    unsafe {
        clang_annotateTokens(tu, tokens_ptr, token_count, ann_cursors.as_mut_ptr());
    }

    // Determine the spelling of the token at the clicked position.
    let mut tgt_file: CXFile = std::ptr::null_mut();
    let mut tgt_line: u32 = 0;
    let mut tgt_col: u32 = 0;
    unsafe {
        clang_getSpellingLocation(
            location,
            &mut tgt_file,
            &mut tgt_line,
            &mut tgt_col,
            std::ptr::null_mut(),
        );
    }
    if tgt_file.is_null() {
        return Ok(None);
    }

    // Find the target token: the identifier whose extent contains the clicked column.
    // Non-identifier tokens (e.g. '[') may also overlap the click column (because
    // clang token end-col is exclusive and equals the next token's start-col), so
    // we skip non-identifiers and continue scanning rather than breaking early.
    let mut target_spelling: Option<String> = None;
    for tok in token_slice.iter() {
        let ext = unsafe { clang_getTokenExtent(tu, *tok) };
        let mut sf: CXFile = std::ptr::null_mut();
        let mut sl: u32 = 0;
        let mut sc: u32 = 0;
        let mut el: u32 = 0;
        let mut ec: u32 = 0;
        let mut _ef: CXFile = std::ptr::null_mut();
        unsafe {
            clang_getSpellingLocation(
                clang_getRangeStart(ext),
                &mut sf,
                &mut sl,
                &mut sc,
                std::ptr::null_mut(),
            );
            clang_getSpellingLocation(
                clang_getRangeEnd(ext),
                &mut _ef,
                &mut el,
                &mut ec,
                std::ptr::null_mut(),
            );
        }
        if sf.is_null() || sf != tgt_file {
            continue;
        }
        if sl == tgt_line && sc <= tgt_col && tgt_col <= ec {
            let spelling = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
            info!(
                "strategy2 target-token scan: tok spell={:?} kind={} sc={} ec={} tgt_col={}",
                spelling,
                unsafe { clang_getTokenKind(*tok) } as u32,
                sc,
                ec,
                tgt_col
            );
            // Only attempt symbol lookup for identifier tokens (kind 2).
            if unsafe { clang_getTokenKind(*tok) } == 2 {
                target_spelling = Some(spelling);
                break; // found the identifier at the click position
            }
            // Non-identifier token overlaps click position — keep scanning
            // (e.g. '[' at col N-1 has end-col == col N, overlapping the
            // start of the identifier that immediately follows).
        }
    }

    info!("strategy2: target_spelling={:?}", target_spelling);
    let target_spelling = match target_spelling {
        Some(s) => s,
        None => return Ok(None),
    };

    // Find any annotated token in the container with the same spelling and a
    // non-null referenced cursor.  Pick the first such token.
    for (tok, ann_cur) in token_slice.iter().zip(ann_cursors.iter()) {
        // Only look at identifier tokens.
        if unsafe { clang_getTokenKind(*tok) } != 2 {
            continue;
        }
        let spelling = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
        if spelling != target_spelling {
            continue;
        }
        info!(
            "strategy2: found twin {:?} ann_cur_kind={}",
            spelling,
            unsafe { clang_getCursorKind(*ann_cur) } as u32
        );
        let ref_c = unsafe { clang_getCursorReferenced(*ann_cur) };
        if unsafe { clang_Cursor_isNull(ref_c) } != 0 {
            continue;
        }
        // Found a resolved twin.  Resolve its definition / declaration.
        let def_c = unsafe { clang_getCursorDefinition(*ann_cur) };
        if unsafe { clang_Cursor_isNull(def_c) } == 0 {
            if let Some(loc) = cursor_to_location(def_c)? {
                return Ok(Some(loc));
            }
        }
        if let Some(loc) = cursor_to_location(ref_c)? {
            return Ok(Some(loc));
        }
    }
    // ── Strategy 3: ObjCMessageExpr descendant walk ────────────────────────
    //
    // Strategy-2 cannot help when the clicked token is a selector component
    // (e.g. `bezierPathWithRect:`), because no other token in the DeclStmt
    // shares its spelling with a resolved annotation twin.
    //
    // Instead: walk ALL descendants of the container looking for an
    // ObjCMessageExpr node (kind=233). `clang_getCursorReferenced()` on an
    // ObjCMessageExpr returns the called method's ObjCClassMethodDecl /
    // ObjCInstanceMethodDecl, giving us a valid goto-def target.
    //
    // We collect only the first ObjCMessageExpr whose source extent contains
    // the clicked position to avoid resolving the wrong message in compound
    // expressions. If no positional match, we fall back to the first
    // ObjCMessageExpr in the container.
    struct MsgExprCtx {
        target: CXSourceLocation,
        best: Option<CXCursor>,
        fallback: Option<CXCursor>,
    }

    extern "C" fn msg_visitor(
        cursor: CXCursor,
        _parent: CXCursor,
        data: CXClientData,
    ) -> CXChildVisitResult {
        let ctx = unsafe { &mut *(data as *mut MsgExprCtx) };
        let kind = unsafe { clang_getCursorKind(cursor) };
        // ObjCMessageExpr = 233
        if kind == 233 {
            // Check if this message expr's extent contains the target position.
            let extent = unsafe { clang_getCursorExtent(cursor) };
            if unsafe { clang_Range_isNull(extent) } == 0 {
                let mut sf: CXFile = std::ptr::null_mut();
                let mut sl: u32 = 0;
                let mut sc: u32 = 0;
                let mut el: u32 = 0;
                let mut ec: u32 = 0;
                let mut ef: CXFile = std::ptr::null_mut();
                unsafe {
                    clang_getSpellingLocation(
                        clang_getRangeStart(extent),
                        &mut sf,
                        &mut sl,
                        &mut sc,
                        std::ptr::null_mut(),
                    );
                    clang_getSpellingLocation(
                        clang_getRangeEnd(extent),
                        &mut ef,
                        &mut el,
                        &mut ec,
                        std::ptr::null_mut(),
                    );
                }
                let mut tf: CXFile = std::ptr::null_mut();
                let mut tl: u32 = 0;
                let mut tc: u32 = 0;
                unsafe {
                    clang_getSpellingLocation(
                        ctx.target,
                        &mut tf,
                        &mut tl,
                        &mut tc,
                        std::ptr::null_mut(),
                    );
                }
                if !sf.is_null() && !tf.is_null() && sf == tf {
                    let in_range = (tl, tc) >= (sl, sc) && (tl, tc) <= (el, ec);
                    if in_range && ctx.best.is_none() {
                        ctx.best = Some(cursor);
                    }
                }
                if ctx.fallback.is_none() {
                    ctx.fallback = Some(cursor);
                }
            }
            // Recurse into nested message expressions.
            return CXChildVisit_Recurse;
        }
        CXChildVisit_Recurse
    }

    let mut msg_ctx = MsgExprCtx {
        target: location,
        best: None,
        fallback: None,
    };
    unsafe {
        clang_visitChildren(
            container,
            msg_visitor,
            &mut msg_ctx as *mut MsgExprCtx as CXClientData,
        );
    }

    let msg_cursor = msg_ctx.best.or(msg_ctx.fallback);
    if let Some(mc) = msg_cursor {
        let ref_c = unsafe { clang_getCursorReferenced(mc) };
        if unsafe { clang_Cursor_isNull(ref_c) } == 0 {
            info!("strategy3: found ObjCMessageExpr ref kind={}", unsafe { clang_getCursorKind(ref_c) } as u32);
            let def_c = unsafe { clang_getCursorDefinition(ref_c) };
            if unsafe { clang_Cursor_isNull(def_c) } == 0 {
                if let Some(loc) = cursor_to_location(def_c)? {
                    return Ok(Some(loc));
                }
            }
            if let Some(loc) = cursor_to_location(ref_c)? {
                return Ok(Some(loc));
            }
        }
    }
    // ── Strategy 4: Token-based receiver-class method lookup ──────────────
    //
    // When Strategy-3 fails because libclang produces a broken AST (e.g. the
    // ObjC message send contains undefined macros as arguments, causing
    // clang_visitChildren to never expose an ObjCMessageExpr node), we fall
    // back to pure token scanning:
    //
    // 1. Scan backward from the clicked selector token to find the '[' bracket.
    // 2. The identifier immediately after '[' is the receiver class name.
    // 3. Find an annotated token with that spelling that resolves to an
    //    ObjCInterfaceDecl or ObjCClassRef (the type-annotation twin).
    // 4. Walk the ObjCInterfaceDecl's children to find a class/instance method
    //    whose selector spelling starts with the clicked token's spelling.
    // 5. Return that method's definition / declaration location.
    //
    // Guard: only invoke when the clicked token is in selector position
    // (not inside a nested parenthesised call like CGRectMake(...)).
    {
        // Find the index of the clicked token in the token list.
        let mut clicked_idx: Option<usize> = None;
        for (i, tok) in token_slice.iter().enumerate() {
            let ext = unsafe { clang_getTokenExtent(tu, *tok) };
            let mut sf: CXFile = std::ptr::null_mut();
            let mut sl: u32 = 0; let mut sc: u32 = 0;
            let mut ec: u32 = 0;
            unsafe {
                clang_getSpellingLocation(clang_getRangeStart(ext), &mut sf, &mut sl, &mut sc, std::ptr::null_mut());
                clang_getSpellingLocation(clang_getRangeEnd(ext), &mut std::ptr::null_mut(), &mut sl, &mut ec, std::ptr::null_mut());
            }
            let mut ext_sl: u32 = 0; let mut ext_sc: u32 = 0;
            unsafe { clang_getSpellingLocation(clang_getRangeStart(ext), &mut sf, &mut ext_sl, &mut ext_sc, std::ptr::null_mut()); }
            if sf.is_null() || sf != tgt_file { continue; }
            let sp = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
            if sp == target_spelling && ext_sl == tgt_line && ext_sc <= tgt_col && tgt_col <= ec {
                clicked_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = clicked_idx {
            // Scan backward to find '[' (Punctuation token, kind=0).
            let mut bracket_idx: Option<usize> = None;
            let mut depth: i32 = 0;
            for i in (0..idx).rev() {
                let tok = token_slice[i];
                let sp = cx_string_owned(unsafe { clang_getTokenSpelling(tu, tok) });
                let kind = unsafe { clang_getTokenKind(tok) }; // 0=Punctuation
                if kind == 0 {
                    if sp == "]" { depth += 1; }
                    else if sp == "[" {
                        if depth == 0 { bracket_idx = Some(i); break; }
                        depth -= 1;
                    }
                }
            }

            if let Some(bi) = bracket_idx {
                // Guard: clicked token must NOT be inside a nested parenthesised
                // expression (e.g. argument to CGRectMake) between '[' and idx.
                let paren_depth: i32 = token_slice[bi..idx].iter().fold(0i32, |d, tok| {
                    if unsafe { clang_getTokenKind(*tok) } == 0 {
                        let sp = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
                        if sp == "(" { d + 1 } else if sp == ")" { d - 1 } else { d }
                    } else { d }
                });
                if paren_depth == 0 {
                    // The receiver is the first identifier token after '['.
                    let receiver_token = (bi + 1..token_slice.len()).find(|&j| {
                        (unsafe { clang_getTokenKind(token_slice[j]) }) == 2 // Identifier
                    });

                    if let Some(ri) = receiver_token {
                        let recv_spelling = cx_string_owned(unsafe { clang_getTokenSpelling(tu, token_slice[ri]) });
                        info!("strategy4: receiver={:?} selector={:?}", recv_spelling, target_spelling);

                        // Find an annotated token with receiver_spelling that has a
                        // non-null ref to an ObjCInterfaceDecl (kind=11) or ObjCClassRef (kind=42).
                        let mut iface_cursor: Option<CXCursor> = None;
                        for (tok, ann_cur) in token_slice.iter().zip(ann_cursors.iter()) {
                            if unsafe { clang_getTokenKind(*tok) } != 2 { continue; }
                            let sp = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
                            if sp != recv_spelling { continue; }
                            let ref_c = unsafe { clang_getCursorReferenced(*ann_cur) };
                            if unsafe { clang_Cursor_isNull(ref_c) } != 0 { continue; }
                            let ref_kind = unsafe { clang_getCursorKind(ref_c) };
                            // ObjCInterfaceDecl=11, ObjCClassRef=42
                            if ref_kind == 11 || ref_kind == 42 {
                                // Resolve to the actual ObjCInterfaceDecl
                                let def_c = unsafe { clang_getCursorDefinition(ref_c) };
                                iface_cursor = Some(if unsafe { clang_Cursor_isNull(def_c) } == 0 { def_c } else { ref_c });
                                break;
                            }
                        }

                        if let Some(iface) = iface_cursor {
                            let iface_spell = cx_string_owned(unsafe { clang_getCursorSpelling(iface) });
                            info!("strategy4: found iface kind={} spell={:?}", unsafe { clang_getCursorKind(iface) } as u32, iface_spell);
                            // Ensure we have the canonical definition (not a forward decl).
                            let iface_def = unsafe { clang_getCursorDefinition(iface) };
                            let iface_walk = if unsafe { clang_Cursor_isNull(iface_def) } == 0 { iface_def } else { iface };
                            info!("strategy4: iface_walk kind={}", unsafe { clang_getCursorKind(iface_walk) } as u32);
                            // Walk the ObjCInterfaceDecl children to find a method
                            // whose selector starts with target_spelling.
                            struct MethodCtx {
                                selector_prefix: String,
                                found: Option<CXCursor>,
                            }
                            extern "C" fn method_visitor(
                                cursor: CXCursor,
                                _parent: CXCursor,
                                data: CXClientData,
                            ) -> CXChildVisitResult {
                                let ctx = unsafe { &mut *(data as *mut MethodCtx) };
                                let kind = unsafe { clang_getCursorKind(cursor) };
                                // ObjCInstanceMethodDecl=16, ObjCClassMethodDecl=17
                                if kind == 16 || kind == 17 {
                                    let sp = cx_string_owned(unsafe { clang_getCursorSpelling(cursor) });
                                    if sp.starts_with(ctx.selector_prefix.as_str()) {
                                        ctx.found = Some(cursor);
                                        return CXChildVisit_Break;
                                    }
                                }
                                CXChildVisit_Continue
                            }
                            let mut method_ctx = MethodCtx {
                                selector_prefix: target_spelling.clone(),
                                found: None,
                            };
                            unsafe {
                                clang_visitChildren(
                                    iface_walk,
                                    method_visitor,
                                    &mut method_ctx as *mut MethodCtx as CXClientData,
                                );
                            }
                            if let Some(method) = method_ctx.found {
                                info!("strategy4: found method {}", cx_string_owned(unsafe { clang_getCursorSpelling(method) }));
                                if let Some(loc) = cursor_to_location(method)? {
                                    return Ok(Some(loc));
                                }
                            }
                        } // if let Some(iface)
                    } // if let Some(ri)
                } // if paren_depth == 0
            } // if let Some(bi)
        } // if let Some(idx)
    } // strategy4 block

    // ── Strategy 5: Whole-file annotated-token twin search ─────────────────
    //
    // For C functions (CGRectMake), macros, or other identifiers on broken-AST
    // lines that strategy2 missed, tokenise the entire source file and search
    // for any token with the same spelling whose annotated cursor resolves to a
    // non-null reference.  This finds CGRectMake declared in UIGeometry.h even
    // when the call site lives on a broken-AST line.
    {
        let file_range = unsafe {
            let start = clang_getLocation(tu, tgt_file, 1, 1);
            let end   = clang_getLocation(tu, tgt_file, u32::MAX / 2, 1);
            clang_getRange(start, end)
        };
        if unsafe { clang_Range_isNull(file_range) } == 0 {
            let mut fp: *mut CXToken = std::ptr::null_mut();
            let mut fc: u32 = 0;
            unsafe { clang_tokenize(tu, file_range, &mut fp, &mut fc); }
            if !fp.is_null() && fc > 0 {
                struct FGuard(*mut CXToken, u32, CXTranslationUnit);
                impl Drop for FGuard {
                    fn drop(&mut self) { unsafe { clang_disposeTokens(self.2, self.0, self.1) }; }
                }
                let _fg = FGuard(fp, fc, tu);
                let ftoks = unsafe { std::slice::from_raw_parts(fp, fc as usize) };
                let mut fann: Vec<CXCursor> = vec![unsafe { std::mem::zeroed() }; fc as usize];
                unsafe { clang_annotateTokens(tu, fp, fc, fann.as_mut_ptr()); }
                for (tok, ann_cur) in ftoks.iter().zip(fann.iter()) {
                    if unsafe { clang_getTokenKind(*tok) } != 2 { continue; }
                    let sp = cx_string_owned(unsafe { clang_getTokenSpelling(tu, *tok) });
                    if sp != target_spelling { continue; }
                    // Skip tokens on the same broken line — they won't be resolved.
                    let tok_loc = unsafe { clang_getTokenLocation(tu, *tok) };
                    let mut tok_file: CXFile = std::ptr::null_mut();
                    let mut tok_line: u32 = 0;
                    unsafe { clang_getSpellingLocation(tok_loc, &mut tok_file, &mut tok_line, std::ptr::null_mut(), std::ptr::null_mut()); }
                    if tok_line == tgt_line { continue; }
                    let ref_c = unsafe { clang_getCursorReferenced(*ann_cur) };
                    if unsafe { clang_Cursor_isNull(ref_c) } != 0 { continue; }
                    // Found a resolved occurrence on a different line.
                    let def_c = unsafe { clang_getCursorDefinition(*ann_cur) };
                    if unsafe { clang_Cursor_isNull(def_c) } == 0 {
                        if let Some(loc) = cursor_to_location(def_c)? {
                            return Ok(Some(loc));
                        }
                    }
                    if let Some(loc) = cursor_to_location(ref_c)? {
                        return Ok(Some(loc));
                    }
                }
            }
        }
    }

    Ok(None)
}
