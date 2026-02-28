//! Detects whether a `.h` file is Objective-C, C, or C++.
//!
//! clangd has had this bug open since 2020 (clangd#621).
//! We detect by scanning the first 8 KB for ObjC-specific tokens.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderLanguage {
    ObjC,
    ObjCPlusPlus,
    C,
    Cpp,
}

/// Infer the language of a `.h` file from its contents.
///
/// Heuristic order (first match wins):
/// 1. If `@interface`, `@protocol`, `@implementation`, `@end` present → ObjC
/// 2. If `class ` / `template<` / `namespace ` / `#include <` with `>` present → C++
/// 3. Otherwise → C
pub fn detect_header_language(path: &Path, source: &str) -> HeaderLanguage {
    // Only scan the first 8 KB for performance.
    let sample = &source[..source.len().min(8192)];

    let is_objcpp = path.extension().map_or(false, |e| e == "mm");

    // ObjC-specific keyword scan.
    let has_objc = sample.contains("@interface")
        || sample.contains("@protocol")
        || sample.contains("@implementation")
        || sample.contains("@end")
        || sample.contains("@property")
        || sample.contains("NS_ASSUME_NONNULL")
        || sample.contains("FOUNDATION_EXPORT")
        || sample.contains("#import ");

    if has_objc {
        return if is_objcpp {
            HeaderLanguage::ObjCPlusPlus
        } else {
            HeaderLanguage::ObjC
        };
    }

    // C++-specific scan.
    let has_cpp = sample.contains("class ")
        || sample.contains("template<")
        || sample.contains("template <")
        || sample.contains("namespace ")
        || sample.contains("::")
        || sample.contains("std::");

    if has_cpp {
        return if is_objcpp {
            HeaderLanguage::ObjCPlusPlus
        } else {
            HeaderLanguage::Cpp
        };
    }

    HeaderLanguage::C
}

/// Convert a `HeaderLanguage` to the clang `-x` flag value.
pub fn to_clang_x_flag(lang: HeaderLanguage) -> &'static str {
    match lang {
        HeaderLanguage::ObjC => "objective-c-header",
        HeaderLanguage::ObjCPlusPlus => "objective-c++-header",
        HeaderLanguage::C => "c-header",
        HeaderLanguage::Cpp => "c++-header",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn h(src: &str) -> HeaderLanguage {
        detect_header_language(&PathBuf::from("test.h"), src)
    }

    #[test]
    fn detects_objc_interface() {
        assert_eq!(h("@interface Foo : NSObject\n@end"), HeaderLanguage::ObjC);
    }

    #[test]
    fn detects_objc_import() {
        assert_eq!(h("#import <Foundation/Foundation.h>"), HeaderLanguage::ObjC);
    }

    #[test]
    fn detects_cpp_class() {
        assert_eq!(h("class Foo { int x; };"), HeaderLanguage::Cpp);
    }

    #[test]
    fn detects_plain_c() {
        assert_eq!(h("typedef struct { int x; } Foo;"), HeaderLanguage::C);
    }

    #[test]
    fn objc_wins_over_cpp_markers() {
        // A mixed ObjC++ header should still be detected as ObjC, not C++.
        assert_eq!(
            h("@interface Foo : NSObject\nclass Bar {};\n@end"),
            HeaderLanguage::ObjC
        );
    }
}
