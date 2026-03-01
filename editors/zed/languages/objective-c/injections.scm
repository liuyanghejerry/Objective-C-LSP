; Objective-C injection rules for Zed
; Based on https://github.com/Akzestia/objcpp (MIT, blacktop)

((comment) @injection.content
 (#set! injection.language "comment")
)

(preproc_def
    value: (preproc_arg) @injection.content
    (#set! injection.language "c"))

(preproc_function_def
    value: (preproc_arg) @injection.content
    (#set! injection.language "c"))
