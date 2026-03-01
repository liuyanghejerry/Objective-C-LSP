; Objective-C tree-sitter highlights for Zed
; Grammar: jiyee/tree-sitter-objc

; ── ObjC keywords ──────────────────────────────────────────────────────────────

"@interface" @keyword
"@implementation" @keyword
"@protocol" @keyword
"@end" @keyword
"@class" @keyword
"@property" @keyword
"@synthesize" @keyword
"@dynamic" @keyword
"@selector" @keyword
"@encode" @keyword
"@protocol" @keyword
"@optional" @keyword
"@required" @keyword
"@public" @keyword
"@private" @keyword
"@protected" @keyword
"@package" @keyword
"@try" @keyword
"@catch" @keyword
"@finally" @keyword
"@throw" @keyword
"@synchronized" @keyword
"@autoreleasepool" @keyword
"@import" @keyword
"@compatibility_alias" @keyword
"@available" @keyword
"@defs" @keyword

; ── C keywords ─────────────────────────────────────────────────────────────────

[
  "break"
  "case"
  "continue"
  "default"
  "do"
  "else"
  "enum"
  "extern"
  "for"
  "goto"
  "if"
  "inline"
  "return"
  "sizeof"
  "struct"
  "switch"
  "typedef"
  "union"
  "while"
] @keyword

"#define" @keyword
"#elif" @keyword
"#else" @keyword
"#endif" @keyword
"#if" @keyword
"#ifdef" @keyword
"#ifndef" @keyword
"#include" @keyword
"#import" @keyword

; ── Storage & type qualifiers ──────────────────────────────────────────────────

[
  "const"
  "static"
  "volatile"
  "register"
  "restrict"
  "extern"
  "inline"
  "_Atomic"
  "__block"
  "__weak"
  "__strong"
  "__unsafe_unretained"
  "__autoreleasing"
] @keyword

; ── Types ──────────────────────────────────────────────────────────────────────

(primitive_type) @type.builtin
(type_identifier) @type
(sized_type_specifier) @type.builtin

; ObjC builtin types
(id) @type.builtin
(instancetype) @type.builtin
(SEL) @type.builtin
(IMP) @type.builtin
(BOOL) @type.builtin
(Class) @type.builtin
(auto) @type.builtin

; ── ObjC declarations ─────────────────────────────────────────────────────────

; @interface ClassName
(class_interface
  name: (identifier) @type)

; @implementation ClassName
(class_implementation
  name: (identifier) @type)

; @protocol ProtocolName
(protocol_declaration
  name: (identifier) @type)

; @interface ClassName (CategoryName)
(category_interface
  name: (identifier) @type
  category: (identifier) @label)

; @implementation ClassName (CategoryName)
(category_implementation
  name: (identifier) @type
  category: (identifier) @label)

; Superclass reference: @interface Foo : Bar
(superclass_reference (identifier) @type)

; Protocol conformance list: <NSCoding, NSCopying>
(protocol_qualifiers (identifier) @type)

; ── Methods ────────────────────────────────────────────────────────────────────

; Method declaration: - (void)doSomething;
(method_declaration
  selector: (identifier) @function.method)

; Method declaration with keyword selectors: - (void)doSomething:(int)x with:(int)y;
(method_declaration
  (keyword_selector
    (keyword_declarator
      keyword: (identifier) @function.method)))

; Method definition: - (void)doSomething { ... }
(method_definition
  selector: (identifier) @function.method)

; Method definition with keyword selectors
(method_definition
  (keyword_selector
    (keyword_declarator
      keyword: (identifier) @function.method)))

; ── Properties ─────────────────────────────────────────────────────────────────

(property_declaration
  name: (identifier) @property)

; Property attributes: nonatomic, strong, readonly, etc.
(property_attributes
  (identifier) @attribute)

; @synthesize and @dynamic
(synthesize_definition
  (synthesize_property
    property: (identifier) @property))

; ── Message expressions ────────────────────────────────────────────────────────

; [obj method]
(message_expression
  selector: (identifier) @function.method)

; [obj method:arg with:arg2]
(message_expression
  (keyword_argument
    keyword: (identifier) @function.method))

; ── @selector / @encode / @protocol expressions ───────────────────────────────

(selector_expression (identifier) @function.method)
(selector_expression
  (keyword_argument
    keyword: (identifier) @function.method))

(encode_expression) @function.builtin
(protocol_expression (identifier) @type)

; ── C functions ────────────────────────────────────────────────────────────────

(call_expression
  function: (identifier) @function)

(function_declarator
  declarator: (identifier) @function)

(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function))

; ── Strings ────────────────────────────────────────────────────────────────────

(string_literal) @string
(string_expression) @string
(char_literal) @string
(concatenated_string) @string
(system_lib_string) @string

(escape_sequence) @string.escape

; ── Numbers ────────────────────────────────────────────────────────────────────

(number_literal) @number
(number_expression) @number

; ── Boolean / nil / null / self / super ────────────────────────────────────────

(YES) @constant.builtin
(NO) @constant.builtin
(true) @constant.builtin
(false) @constant.builtin
(nil) @constant.builtin
(null) @constant.builtin
(self) @variable.special
(super) @variable.special

; ── Identifiers ────────────────────────────────────────────────────────────────

(identifier) @variable
(field_identifier) @property
(field_expression
  field: (field_identifier) @property)
(statement_identifier) @label

; ── Preprocessor ───────────────────────────────────────────────────────────────

(preproc_def
  name: (identifier) @constant)
(preproc_function_def
  name: (identifier) @function)
(preproc_include
  path: (string_literal) @string)
(preproc_include
  path: (system_lib_string) @string)
(preproc_import
  path: (string_literal) @string)
(preproc_import
  path: (system_lib_string) @string)

; ── Comments ───────────────────────────────────────────────────────────────────

(comment) @comment

; ── Operators ──────────────────────────────────────────────────────────────────

[
  "+"
  "-"
  "*"
  "/"
  "%"
  "&"
  "|"
  "^"
  "~"
  "!"
  "&&"
  "||"
  "<<"
  ">>"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "="
  "+="
  "-="
  "*="
  "/="
  "%="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "++"
  "--"
  "->"
  "."
  "?"
  ":"
] @operator

; ── Punctuation ────────────────────────────────────────────────────────────────

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
[";" "," "..."] @punctuation.delimiter
