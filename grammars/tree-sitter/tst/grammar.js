/**
 * @file Tree-sitter grammar for Nand to Tetris test scripts (.tst).
 * @license MPL-2.0
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/** A case-insensitive keyword, exposed as the lower-case literal `word`. */
const kw = (word) =>
  alias(
    token(
      prec(
        2,
        new RegExp(
          word
            .split('')
            .map((c) => (/[a-z]/.test(c) ? `[${c}${c.toUpperCase()}]` : `\\${c}`))
            .join(''),
        ),
      ),
    ),
    word,
  );

const STRING = /"([^"\\\r\n]|\\.)*"/;

module.exports = grammar({
  name: 'tst',

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  rules: {
    source_file: ($) => repeat(choice($.command, $.repeat_block, $.while_block)),

    command: ($) =>
      seq(
        field(
          'body',
          choice(
            $.load_command,
            $.output_file_command,
            $.compare_to_command,
            $.output_list_command,
            $.set_command,
            $.echo_command,
            $.breakpoint_command,
            $.simple_command,
            $.builtin_call,
          ),
        ),
        field('terminator', $.terminator),
      ),

    terminator: (_) => choice(',', ';', '!'),

    load_command: ($) => seq(kw('load'), field('file', $._file)),
    output_file_command: ($) => seq(kw('output-file'), field('file', $._file)),
    compare_to_command: ($) => seq(kw('compare-to'), field('file', $._file)),

    output_list_command: ($) => seq(kw('output-list'), repeat1($.output_item)),

    output_item: ($) => seq(field('variable', $.variable), optional(field('format', $.format))),

    set_command: ($) =>
      seq(kw('set'), field('variable', $.variable), field('value', $._integer)),

    echo_command: ($) => seq(kw('echo'), field('message', $.string)),

    breakpoint_command: ($) =>
      seq(kw('breakpoint'), field('variable', $.variable), field('value', $._scalar)),

    simple_command: (_) =>
      choice(
        kw('eval'),
        kw('output'),
        kw('tick'),
        kw('tock'),
        kw('clear-echo'),
        kw('clear-breakpoints'),
        kw('ticktock'),
        kw('vmstep'),
      ),

    builtin_call: ($) =>
      seq(
        field('chip', $.identifier),
        field('method', $.identifier),
        repeat(field('argument', choice($.file_name, $.string, $.number, $.radix_number))),
      ),

    repeat_block: ($) =>
      seq(kw('repeat'), optional(field('count', $.number)), field('body', $.block)),

    while_block: ($) => seq(kw('while'), field('condition', $.condition), field('body', $.block)),

    block: ($) => seq('{', repeat($.command), '}'),

    condition: ($) =>
      seq(field('left', $._operand), field('relation', $.relation), field('right', $._operand)),

    relation: (_) => choice('=', '<>', '<', '>', '<=', '>='),

    _operand: ($) => choice($.variable, $._scalar),

    _scalar: ($) => choice($._integer, $.percent_string),

    _integer: ($) => choice($.number, $.radix_number),

    variable: ($) => seq(field('name', $.identifier), optional(field('index', $.index))),

    index: ($) => seq('[', optional($.number), ']'),

    _file: ($) => choice($.file_name, $.string),

    file_name: (_) => /[A-Za-z0-9_][A-Za-z0-9_.\-]*/,

    format: (_) => token(/%[BbDdXxSs][0-9]+\.[0-9]+\.[0-9]+/),

    radix_number: (_) => token(/%[BbDdXx]-?[0-9A-Fa-f]+/),

    percent_string: (_) => token(seq(/%[Ss]/, /[ \t]*/, STRING)),

    string: (_) => token(STRING),

    number: (_) => /-?[0-9]+/,

    identifier: (_) => /[A-Za-z_][A-Za-z0-9_]*/,

    line_comment: (_) => token(seq('//', /[^\r\n]*/)),

    block_comment: (_) => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
