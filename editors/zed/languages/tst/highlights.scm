; Commands
[
  "load" "output-file" "compare-to" "output-list" "set" "echo" "breakpoint"
  "eval" "output" "tick" "tock" "clear-echo" "clear-breakpoints"
] @keyword

; Other-dialect commands (CPU / VM emulator)
["ticktock" "vmstep"] @keyword

; Control flow
["repeat" "while"] @keyword

; Built-in method calls: `ROM32K load Max.hack`
(builtin_call chip: (identifier) @type)
(builtin_call method: (identifier) @function)

; Variables
((variable name: (identifier) @variable.special)
  (#match? @variable.special "^[tT][iI][mM][eE]$"))
((variable name: (identifier) @variable)
  (#not-match? @variable "^[tT][iI][mM][eE]$"))

; Literals
(file_name) @string.special
(string) @string
(percent_string) @string
(number) @number
(radix_number) @number
(format) @attribute

; Comments
(line_comment) @comment
((block_comment) @comment.doc
  (#match? @comment.doc "^/\\*\\*[^/]"))
((block_comment) @comment
  (#not-match? @comment "^/\\*\\*[^/]"))

; Punctuation
(relation) @operator
(terminator "!" @punctuation.special)
(terminator ["," ";"] @punctuation.delimiter)
["{" "}" "[" "]"] @punctuation.bracket
