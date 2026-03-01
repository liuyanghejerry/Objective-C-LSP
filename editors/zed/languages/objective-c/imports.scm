; Objective-C import queries for Zed
; Based on https://github.com/Akzestia/objcpp (MIT, blacktop)

(preproc_include
    path: [
        (
            (system_lib_string) @source @wildcard
            (#strip! @source "[<>]"))
        (string_literal (string_content) @source @wildcard)
    ]) @import
