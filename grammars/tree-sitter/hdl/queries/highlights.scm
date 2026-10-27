; Keywords
["CHIP" "IN" "OUT" "PARTS" "BUILTIN" "CLOCKED"] @keyword

; Chip types
(chip_definition name: (identifier) @type)
(part chip: (identifier) @type)
(builtin_section name: (identifier) @function)

; Pins and signals
(pin_declaration name: (identifier) @variable.parameter)
(clocked_clause (identifier) @variable.parameter)
(connection pin: (pin_reference name: (identifier) @property))
(connection signal: (pin_reference name: (identifier) @variable))

; Literals
(boolean) @boolean
(number) @number

; Comments
(line_comment) @comment
((block_comment) @comment.doc
  (#match? @comment.doc "^/\\*\\*[^/]"))
((block_comment) @comment
  (#not-match? @comment "^/\\*\\*[^/]"))

; Punctuation and operators
["=" ".."] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ";" ":"] @punctuation.delimiter
