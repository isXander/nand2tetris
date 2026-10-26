(repeat_block
  body: (block
    "{"
    (_)* @function.inside
    "}")) @function.around

(while_block
  body: (block
    "{"
    (_)* @function.inside
    "}")) @function.around

(line_comment)+ @comment.around
(block_comment) @comment.around
