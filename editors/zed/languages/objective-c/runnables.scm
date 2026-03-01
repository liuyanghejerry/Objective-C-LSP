; Objective-C runnable queries for Zed
; Based on https://github.com/Akzestia/objcpp (MIT, blacktop)

; Tag the main function
(
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @run
    )
  ) @_c-main
  (#eq? @run "main")
  (#set! tag c-main)
)
