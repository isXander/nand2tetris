(chip_definition
  "CHIP" @context
  name: (identifier) @name) @item

(input_section
  (pin_declaration
    name: (identifier) @name) @item)

(output_section
  (pin_declaration
    name: (identifier) @name) @item)

(part
  chip: (identifier) @name) @item

(builtin_section
  "BUILTIN" @context
  name: (identifier) @name) @item
