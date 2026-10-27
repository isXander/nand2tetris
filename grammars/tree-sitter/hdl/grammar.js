/**
 * @file Tree-sitter grammar for Nand to Tetris HDL (.hdl).
 * @license MPL-2.0
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const commaSep1 = (rule) => seq(rule, repeat(seq(',', rule)));

module.exports = grammar({
  name: 'hdl',

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat($.chip_definition),

    chip_definition: ($) =>
      seq(
        'CHIP',
        field('name', $.identifier),
        field('body', $.chip_body),
      ),

    chip_body: ($) =>
      seq(
        '{',
        optional($.input_section),
        optional($.output_section),
        optional(choice($.parts_section, $.builtin_section)),
        '}',
      ),

    input_section: ($) => seq('IN', commaSep1($.pin_declaration), ';'),

    output_section: ($) => seq('OUT', commaSep1($.pin_declaration), ';'),

    pin_declaration: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq('[', field('width', $.number), ']')),
      ),

    parts_section: ($) => seq('PARTS', ':', repeat($.part)),

    part: ($) =>
      seq(
        field('chip', $.identifier),
        field('connections', $.connection_list),
        ';',
      ),

    connection_list: ($) => seq('(', optional(commaSep1($.connection)), ')'),

    connection: ($) =>
      seq(
        field('pin', $.pin_reference),
        '=',
        field('signal', choice($.pin_reference, $.boolean)),
      ),

    pin_reference: ($) =>
      seq(field('name', $.identifier), optional(field('selector', $.selector))),

    selector: ($) =>
      seq('[', field('start', $.number), optional(seq('..', field('end', $.number))), ']'),

    builtin_section: ($) =>
      seq('BUILTIN', field('name', $.identifier), ';', optional($.clocked_clause)),

    clocked_clause: ($) => seq('CLOCKED', optional(commaSep1($.identifier)), ';'),

    boolean: (_) => choice('true', 'false'),

    identifier: (_) => /[A-Za-z_][A-Za-z0-9_]*/,

    number: (_) => /[0-9]+/,

    line_comment: (_) => token(seq('//', /[^\r\n]*/)),

    block_comment: (_) =>
      token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
