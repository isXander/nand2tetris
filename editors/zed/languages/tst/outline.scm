(load_command
  "load" @context
  file: (_) @name) @item

(output_list_command
  "output-list" @context) @item

(repeat_block
  "repeat" @context
  count: (number)? @name) @item

(while_block
  "while" @context
  condition: (condition) @name) @item

(command
  body: (echo_command
    message: (string) @name)) @item
