; Objective-C code outline for Zed

; @interface ClassName
(class_interface
  name: (identifier) @name) @item

; @implementation ClassName
(class_implementation
  name: (identifier) @name) @item

; @protocol ProtocolName
(protocol_declaration
  name: (identifier) @name) @item

; @interface ClassName (CategoryName)
(category_interface
  name: (identifier) @name
  category: (identifier) @context) @item

; @implementation ClassName (CategoryName)
(category_implementation
  name: (identifier) @name
  category: (identifier) @context) @item

; Method declarations: - (void)doSomething;
(method_declaration
  selector: (identifier) @name) @item

(method_declaration
  (keyword_selector
    (keyword_declarator
      keyword: (identifier) @name))) @item

; Method definitions: - (void)doSomething { ... }
(method_definition
  selector: (identifier) @name) @item

(method_definition
  (keyword_selector
    (keyword_declarator
      keyword: (identifier) @name))) @item

; Properties: @property (nonatomic, strong) NSString *name;
(property_declaration
  name: (identifier) @name) @item

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
