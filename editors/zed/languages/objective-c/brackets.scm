; Objective-C bracket matching for Zed
; Based on https://github.com/Akzestia/objcpp (MIT, blacktop)

("(" @open ")" @close)
("[" @open "]" @close)
("{" @open "}" @close)
("\"" @open "\"" @close)
("'" @open "'" @close)
