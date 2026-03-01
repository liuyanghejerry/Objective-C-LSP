; Objective-C code outline for Zed
; Grammar: tree-sitter-grammars/tree-sitter-objc

; @interface ClassName
(class_interface
  (identifier) @name) @item

; @implementation ClassName
(class_implementation
  (identifier) @name) @item

; @protocol ProtocolName
(protocol_declaration
  (identifier) @name) @item

; Method declarations: - (void)doSomething;
(method_declaration
  (identifier) @name) @item

; Method definitions: - (void)doSomething { ... }
(method_definition
  (identifier) @name) @item

; C functions
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @item

; Struct declarations
(struct_specifier
  name: (type_identifier) @name) @item

; Enum declarations
(enum_specifier
  name: (type_identifier) @name) @item

; Typedefs
(type_definition
  declarator: (type_identifier) @name) @item
