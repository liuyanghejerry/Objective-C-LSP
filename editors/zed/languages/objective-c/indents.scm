; Objective-C auto-indentation rules for Zed

; Indent after opening braces
(compound_statement "{" @indent)
(compound_statement "}" @outdent)

; ObjC class/protocol/category bodies
(class_interface "@interface" @indent)
(class_interface "@end" @outdent)

(class_implementation "@implementation" @indent)
(class_implementation "@end" @outdent)

(protocol_declaration "@protocol" @indent)
(protocol_declaration "@end" @outdent)

(category_interface "@interface" @indent)
(category_interface "@end" @outdent)

(category_implementation "@implementation" @indent)
(category_implementation "@end" @outdent)

; Control flow
(if_statement ")" @indent)
(else_clause "else" @indent)
(while_statement ")" @indent)
(do_statement "do" @indent)
(for_statement ")" @indent)
(for_in_statement ")" @indent)
(switch_statement ")" @indent)

; Case/default labels
(case_statement ":" @indent)

; @try / @catch / @finally
(try_catch_statement "@try" @indent)

; @autoreleasepool
(autoreleasepool_statement "@autoreleasepool" @indent)

; @synchronized
(synchronized_statement ")" @indent)
