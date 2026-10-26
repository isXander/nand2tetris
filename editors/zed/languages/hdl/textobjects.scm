(chip_definition
  body: (chip_body
    "{"
    (_)* @class.inside
    "}")) @class.around

(part
  connections: (connection_list
    "("
    (_)* @function.inside
    ")")) @function.around

(line_comment)+ @comment.around
(block_comment) @comment.around
