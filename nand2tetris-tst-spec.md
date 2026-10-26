# Nand2Tetris test script specification

Version 1.0 · 26 October 2026 · Hardware Simulator profile, with CPU/VM dialect boundaries

Sibling specifications: [nand2tetris-hdl-spec.md](nand2tetris-hdl-spec.md) and [integration-spec.md](integration-spec.md). The existing HDL document's exact basename is intentional; do not rename its references to `hdl-spec.md`.

## 1. Authority, evidence, and conformance

The primary source is Noam Nisan and Shimon Schocken, *The Elements of Computing Systems*, second edition (MIT Press, 2021), Appendix 3, **Test Description Language**. This document was checked against the supplied 427-page PDF, including its embedded figures. Page references below are **one-based PDF page numbers**, not printed page numbers. Appendix 3 occupies PDF pages 392–409; the hardware dialect is A3.1–A3.2, pages 393–403.

Rules carry the following labels:

- **[Book]**: explicitly documented in the textbook, with a section/page locator.
- **[Derived]**: a consequence of documented rules, rather than an additional textbook quotation.
- **[Reference]**: verified by reading the official legacy Java simulator source at commit `327b7e4337b31efb815e1c1e3913c0efd299bcf2`. This is source inspection, not a claim that a simulator binary was executed.
- **[Decision]**: the implementation contract chosen here where the book is incomplete or inconsistent. These decisions are normative for an implementation using these repository specs, but are not represented as textbook requirements.

**[Decision]** The default `book-hardware` profile implements A3.1–A3.2, using the explicit decisions below. A separately selected `legacy-java` profile applies the differences in §12. A capability manifest must state supported profiles and built-in APIs. Do not claim universal Nand2Tetris compatibility from parsing `.tst` alone. CPU and VM execution are separate optional backends (§10).

**[Decision]** `MUST`, `SHOULD`, and `MAY` in a decision mean required, recommended, and optional implementation behavior. They do not elevate the decision to a textbook fact. For conflicting evidence, retain both descriptions and apply the selected profile; do not silently combine incompatible behaviors.

## 2. Files and execution model

**[Book: A3.1, pp. 393–395]** A testing session involves a tested module, a `.tst` script, optionally an `.out` output file, and optionally a `.cmp` expected-results file. Matching stems are recommended, not required. In particular, `ComputerMax.tst` may load `Computer.hdl` and use `ComputerMax.cmp`.

**[Book]** The files should be kept in the same folder. The reference environment's “current folder” means the folder from which the last file was opened. Its controls execute the script, which in turn controls the tested module.

**[Decision]** For reproducible command-line runs, establish an immutable session `projectDir = parent(canonicalScriptPath)` when opening a script. Resolve script companion files against that directory, not the process's changing working directory. See [integration-spec.md](integration-spec.md), §3, for interactive loads and chip resolution. Never infer or automatically run an adjacent `.cmp` merely because it exists.

**[Book: A3.2, pp. 398–400]** Commands execute in written order. `output` records an observation; it does not itself request `eval`, `tick`, or `tock`. The separators define user-visible simulation steps, not hardware time units.

**[Decision]** A run owns its program counter, loop frame, loaded chip instance, clock phase, breakpoints, output configuration, open streams, and comparison counters. Do not share mutable run state between tests. Parse the complete script before executing it; bind chip-dependent variable references after the relevant `load` executes.

## 3. Lexical rules and grammar

### 3.1 Lexical rules

**[Book: A3.1, p. 394]** Whitespace and these comments are ignored outside literals:

```tst
// through the end of the line
/* through the closing delimiter */
/** documentation comment, with the same parsing effect */
```

**[Book]** Test scripts are case-insensitive except for file and folder names. This differs from HDL, which is case-sensitive.

**[Decision]** Preserve every source spelling and location. Match command words and format letters with ASCII case folding in `book-hardware`; preserve literal text and path bytes. For HDL pin and built-in names, prefer an exact spelling, otherwise accept a unique case-folded match. Report ambiguity if several HDL names match. This bridge is necessary because the book does not explain the collision between case-insensitive scripts and HDL names such as `a` and `A`. Reserved `time` cannot be shadowed in scripts.

**[Decision]** Accept UTF-8 input, an optional initial BOM, LF or CRLF, and horizontal tabs as whitespace. Portable identifiers follow the HDL document's identifier policy; built-in names and variables come from registered descriptors. Block comments do not nest. Unterminated comments, strings, or invalid encodings are errors. A comment delimiter inside a quoted string is literal text.

**[Book: A3.2, pp. 396–397, 400]** Integer prefixes are `%D`, `%B`, and `%X`. The default is decimal. A string value uses `%S` and double quotes. `echo` uses double-quoted text without requiring `%S`.

**[Decision]** Recognize `%S"text"` as one string literal; also accept whitespace between `%S` and the opening quote. In quoted strings support `\"`, `\\`, `\n`, `\r`, and `\t`; reject other escapes. Escape behavior is not specified by the book. A portable script should use plain single-line strings without escapes. Bare paths may contain dots and hyphens and must be lexed in the appropriate argument context. Quoted file names are accepted for spaces as a local convenience; `load` still accepts a basename only.

### 3.2 Concrete grammar

This EBNF is a **[Decision]** formalization. `{ X }` denotes repetition, `[ X ]` optional text, and quoted braces are literal script characters. Token boundaries and command arities are significant even though whitespace is otherwise ignored.

```ebnf
script         = { statement }, EOF ;
statement      = simple, terminator | repeat_block | while_block ;
terminator     = "," | ";" | "!" ;

simple         = "load", file
               | "output-file", file
               | "compare-to", file
               | "output-list", output_item, { output_item }
               | "set", variable, integer
               | "eval" | "output" | "tick" | "tock"
               | "echo", quoted_string
               | "clear-echo"
               | "breakpoint", variable, scalar
               | "clear-breakpoints"
               | builtin_name, method_name, { method_argument } ;

repeat_block   = "repeat", [ unsigned_decimal ], "{", body, "}" ;
while_block    = "while", condition, "{", body, "}" ;
body           = simple, terminator, { simple, terminator } ;
condition      = operand, relation, operand ;
relation       = "=" | ">" | "<" | ">=" | "<=" | "<>" ;
operand        = variable | scalar ;
scalar         = integer | string_value ;

output_item    = variable, [ "%", format, uint, ".", uint, ".", uint ] ;
format         = "B" | "D" | "X" | "S" ;
variable       = identifier | builtin_name, "[", [ uint ], "]" ;
integer        = [ "%D" ], [ "-" ], decimal_digits
               | "%B", binary_digits
               | "%X", hexadecimal_digits ;
string_value   = "%S", quoted_string ;
```

`uint` is a nonnegative decimal integer. `file` is a context-sensitive bare filename or quoted filename. Method argument types and count are supplied by the built-in API, not by the generic grammar. An unknown bare word in a condition is a variable reference, not an implicit string constant.

**[Book + Reference]** `output-list` items are separated by whitespace. Although the book's prose uses “v1, v2, …” as list notation, its actual examples and the reference parser use `output-list a b out;`. A comma ends the command; do not consume it as an item separator.

**[Decision, matching Reference]** A loop body must be nonempty. There is no terminator after `{` or `}`; every simple command inside must have its own terminator. A closing `}` does not add a step or time unit. Nested `repeat`/`while` blocks are rejected in both baseline profiles. The book does not specify nesting; the inspected Java parser explicitly disallows it. Supporting nesting requires a declared extension and an expanded `body` grammar.

**[Decision]** No arithmetic expressions, local variable declarations, assignments other than `set`, `if`, `for`, `break`, `continue`, or functions are implied. Reject `==`, `!=`, `&&`, `||`, HDL `true`/`false` value syntax, `0b`/`0x` prefixes, and pin slices such as `in[0..3]`. A future extension must not silently change baseline parsing.

## 4. Values, variables, and built-in access

### 4.1 Integers and widths

**[Book: A3.2, pp. 396–397, 399]** Integers are represented in two's complement. For a 16-bit destination, the book equates `%B1111111111111111`, `%XFFFF`, `%D-1`, and `-1`. `set` requires compatible value and destination widths. The appendix does not fully define overflow or literal-width rules.

**[Decision]** Store circuit values as width-tagged bit vectors. For the baseline Hack backend, widths are 1–16 bits; a wider generic HDL backend is a separately advertised capability. Never use an unbounded host integer as the circuit's semantic value.

For a writable destination of width `w`:

- Decimal: accept `-2^(w-1) <= n <= 2^w-1` when `w < 16`; accept `-32768 <= n <= 32767` when `w = 16`. Encode accepted negative values modulo `2^w`. This deliberately permits `set load -1` to mean its one-bit pattern while retaining the reference's signed 16-bit decimal domain.
- Binary and hexadecimal: parse unsigned digits into a bit pattern, reject more than 16 binary digits or four hex digits, and require the numeric pattern to fit `w`. Short patterns are zero-extended, not sign-extended. Thus `%XFF` into 16 bits is 255, while `%XFFFF` is the pattern representing -1.
- Leading zero digits do not add significant bits for destination-fit checks, subject to those lexical length limits. Reject empty digits, malformed digits, and overflow; never silently discard significant bits during `set`.
- A 16-bit variable read numerically is signed; narrower pin values are unsigned. A built-in descriptor may declare an unsigned address domain separately. The formatting rules in §7 use the resulting scalar.

**[Decision]** Parse condition and breakpoint constants without assuming an assignment destination: bare decimal denotes its signed mathematical integer; a 16-bit binary/four-digit hexadecimal pattern has the corresponding signed 16-bit value; shorter patterns are nonnegative. Values outside the baseline 16-bit literal domain are errors. Repeat counts and indices are separate nonnegative control integers and are not narrowed to circuit words. `time` is not stored in a 16-bit word.

**[Reference caveat]** The old simulator routes assignments through signed Java `short` conversion, and its check for negative values on narrow pins is unusually permissive. Do not reproduce that defect as a general width rule. Exact behavior for such invalid/nonportable assignments requires a separate compatibility test.

### 4.2 Variables and permissions

**[Book: A3.2, pp. 397, 399, 401]** Scripts can refer to the loaded chip's input, output, and internal pins, built-in state variables, and read-only `time`. Built-in references have forms such as `RAM16K[1017]` and `DRegister[]`.

**[Decision, matching Reference for pin permissions]** Top-level inputs are readable and writable. Top-level outputs and internal wires are readable and read-only. `time` is read-only. The book describes `set` broadly, but that description does not authorize injecting drivers into calculated output/internal wires. Built-in state is writable only where its descriptor provides a setter. Unknown names, ambiguous names, illegal indices, and attempts to write read-only values fail.

**[Decision]** `RAM16K[3]` selects memory word 3; `DRegister[]` selects scalar private state. Neither syntax denotes bit 3 of an HDL pin. Baseline scripting does not implement arbitrary hierarchical paths or user-assigned instance names, which HDL does not declare. If two loaded built-in instances have the same script-visible chip name, report ambiguity rather than choosing an arbitrary instance. The legacy lookup instead returns the first matching GUI-capable built-in.

### 4.3 Required built-in state API

The following table records **[Book: figure A3.2, p. 397]** and the selected **[Decision]** contract. It applies when that chip is a registered built-in and present as the loaded chip or an accessible descendant; it is not an automatic API for an HDL implementation with the same name.

| Reference | Contract | Access / methods |
|---|---|---|
| `Register[]` | One 16-bit state value | Read/write; see timing caveat below |
| `ARegister[]`, `DRegister[]` | One 16-bit state value each | Read/write |
| `PC[]` | One 16-bit counter state | Read/write; correction below |
| `RAM8[i]` | 8 words, `0 <= i < 8` | Read/write, 16-bit entries |
| `RAM64[i]` | 64 words | Read/write, 16-bit entries |
| `RAM512[i]` | 512 words | Read/write, 16-bit entries |
| `RAM4K[i]` | 4096 words | Read/write, 16-bit entries |
| `RAM16K[i]` | 16384 words | Read/write, 16-bit entries |
| `ROM32K[i]` | 32768 instruction words | Read; populate through `load` below |
| `Screen[i]` | 8192 words, `0 <= i < 8192` | Read/write, 16-bit entries; correction below |
| `Keyboard[]` | One 16-bit key code | Script read-only; host input supplies changes |

**Source corrections and limits:**

- Figure A3.2 prints `PC[]` as 15-bit (0–32767). The standalone PC chip has 16-bit input/output and counter state; the CPU's exposed `pc` address is 15-bit. Use the actual chip interface, not that table entry, to size the PC circuit. Chapter 3, chapter 5, and the official `PC.hdl`/`PC.java` support this correction.
- Figure A3.2 prints Screen indices through 16383. Chapter 5 specifies an 8K screen memory, and official `Screen.hdl` declares a 13-bit address. Use indices 0–8191. Do not allocate a 16K Screen from that table entry.
- Figure A3.2 lists `Register[]`, but the inspected Java `Register` class is not GUI-capable, and the legacy script lookup only exposes GUI-capable built-ins. `book-hardware` supplies the documented accessor; `legacy-java` must not promise it for that class.
- ROM's table entry does not clearly define direct script-write permission. **[Decision]** baseline ROM state is read-only through `set`; `ROM32K load` is its initialization API. The inspected reference has a setter; that is a compatibility difference.

**[Book: p. 401]** The example `set Register[] 135` says its output commits to this value in the next time unit. **[Reference]** GUI register setters for ARegister/DRegister, and the PC setter, instead update their outputs immediately and re-evaluate their parent; direct RAM/Screen writes similarly update memory and evaluation. **[Decision]** Every built-in descriptor must specify debug-write timing independently of normal clocked input behavior. `book-hardware` implements `Register[]` as a pending-state edit published by the next `tock`; a later `tick` may replace it according to normal inputs. ARegister/DRegister/PC and RAM/Screen debug writes use the reference's immediate edit behavior. These are deliberate per-chip rules, not a universal consequence of `set`. See the shared clock contract in [integration-spec.md](integration-spec.md), §6.

## 5. Commands

The commands below are **[Book: A3.2, pp. 398–402]** except where a decision or reference refinement is stated. Every simple command requires `,`, `;`, or `!`.

### 5.1 Setup and observations

| Command | Required meaning |
|---|---|
| `load Xxx.hdl` | Load the named HDL chip, searching the current project folder first, then the supplied built-in chip library. The `.hdl` suffix is required; a path specification is forbidden. |
| `output-file Xxx.out` | Create the output file in the current folder for subsequent output. The `.out` suffix is required. |
| `compare-to Xxx.cmp` | Open the expected-results file in the current folder. The `.cmp` suffix is required. Each generated line is checked as specified in §8. |
| `output-list a b out` | Replace the active output schema; emit a header immediately. Each item optionally carries a formatting suffix. The schema remains active until replaced. |
| `output` | Read the schema's current values, format one row, write it, compare it if enabled, then advance the corresponding cursors. No implicit evaluation or clock advancement. |

**[Decision]** Validate suffixes exactly as file spellings, and reject separators `/` and `\`, drive prefixes, and traversal components in baseline companion basenames. The command-line script argument may itself be a full path. `output-file` opens with truncation, not append; repeating it closes the old stream and starts a new file. `compare-to` closes the old compare stream and starts at physical line 1. Neither command resets circuit time. Validate that output cannot overwrite the script, source HDL, compare file, or loaded program. This is an API invariant, not a confirmation dialog.

**[Decision]** `output-list` requires an output stream and at least one item. `output` requires an output stream and an executed output-list. Name binding is validated when the schema is installed and again after a later `load` changes the chip. No implicit default output filename, schema, or compare file is supplied. On `load`, replace circuit and clock state (time 0, phase before tick), invalidate bound variable handles, and clear breakpoints; retain open output/compare streams and their positions and retain the schema's source references for rebinding. These reset choices are explicit session policy.

### 5.2 Hardware actions

| Command | Required meaning |
|---|---|
| `set variable value` | Write a permitted pin or built-in state variable, with width/type/index validation. It does not advance time. |
| `eval` | Evaluate the circuit's combinational logic using current inputs and published state; do not capture or advance sequential state. |
| `tick` | Finish the first half of the current clock cycle; capture state according to the circuit's clocked paths. Time text becomes `t+`. |
| `tock` | Finish the second half, publish captured state, and begin cycle `t+1`. |

**[Decision, matching Reference for phase order]** A newly loaded chip starts at time 0 before `tick`. `tock` before `tick`, repeated `tick`, and repeated `tock` are phase errors, not harmless aliases. Both clock actions must perform the combinational work needed by their semantics; scripts need not add an `eval` between valid `tick`/`tock` commands. `set` on a top-level input can leave calculated pins stale until explicit `eval` or a clock action. `output` observes the published values as they stand.

**[Book: A2.4 and A3.2]** Combinational logic has no clock delay; RAM addressing remains combinational even though writes are clocked. Do not interpret “all state commits at tock” as permission to delay every output of a mixed sequential/combinational chip.

**[Decision]** Default RAM/Screen pin reads use committed memory until tock, even if their inspection API shows pending captured data. The inspected legacy implementation can expose captured backing storage through a recomputed asynchronous read after tick. This edge case and the selected profile-specific behavior are detailed in [integration-spec.md](integration-spec.md), §6.3.

### 5.3 Loops and conditions

**[Book: p. 400]** `repeat n { ... }` executes its body `n` times. Without `n`, repetition continues until stopped. `while x op y { ... }` repeats while its condition is true. Both operands may be variables or constants. Numeric relations are `=`, `>`, `<`, `>=`, `<=`, `<>`; strings support only `=` and `<>`.

**[Decision, matching Reference for repeat bounds]** An explicit count must be a positive decimal integer. `repeat 0` is an error, not an infinite loop or zero iterations. Omission is represented distinctly from zero. Check `while` before the first iteration and at the boundary before each subsequent iteration, reading live variables each time. A false initial condition executes no body commands. Loop transitions do not add clock cycles or output rows.

**[Decision]** Reject mixed integer/string comparisons, string ordering, undeclared names, and implicit conversion of `time` to an integer. The time display is a string (`0`, `0+`, `1`, …); use `%S` for output or explicit string equality conditions. A future numeric cycle-counter API must have a different declared name. The book permits string tests but does not specify ordering for half-cycle times.

### 5.4 Status and breakpoints

**[Book: pp. 400–401]** `echo "text"` replaces the status message; `clear-echo` clears it. `breakpoint variable value` requests suspension when the variable equals the value, checking after subsequent script commands. `clear-breakpoints` removes all breakpoints.

**[Decision]** Echo is a status event, never an `.out` row. De-duplicate identical breakpoint pairs. In `book-hardware`, check enabled breakpoints after each successful simple command following registration. Stop before the next command; preserve loop and stream positions. To make resume useful, a triggered pair re-arms after its equality condition becomes false. `!` and breakpoints suspend; they are not test success and are not runtime errors. A batch API returns `Suspended` with the reason and cursor unless explicitly configured by its caller to continue. It must never silently skip pauses or loop forever while pretending to have completed.

**[Reference difference]** The inspected Java controller checks breakpoints after an entire semicolon-delimited simulation step, and latches a reached breakpoint until its value changes. Its GUI-dependent breakpoint/status code must not be copied into a headless implementation without adaptation.

### 5.5 Built-in methods

**[Book: pp. 401–403, figure A3.2]** The command shape is `builtInChipName method arguments`. For example:

```tst
ROM32K load Max.hack;
```

The figure also documents loading `.asm` into ROM32K. This is a method of an accessible built-in chip, not the top-level `load Xxx.hdl` command.

**[Decision]** Resolve the target through the built-in descriptor and validate its method signature. For `ROM32K load file`, resolve a companion `.hack` or `.asm` basename in `projectDir`; `.asm` requires an explicitly provided Hack assembler. Parse the full program before replacing ROM contents. Reject more than 32768 words and malformed binary words; zero-fill unused locations. Do not reset clock or unrelated state. No arbitrary host-language method invocation, shell command, or Java class loading is part of this language contract. Unsupported methods produce `UnsupportedCapability` or `UnknownMethod`, not a successful no-op.

## 6. Stepping, stops, and completion

**[Book: p. 398]** A comma terminates a command. A semicolon terminates a command and a simulation step. An exclamation mark terminates a command and stops execution until resumed.

**[Decision]** Preserve terminators in the AST or execution plan. Execute the command before applying its terminator:

1. Perform the simple command and any resulting output comparison.
2. On an error or mismatch, stop before any following command.
3. Apply the breakpoint check defined by the selected profile.
4. For `!`, suspend with the next-command cursor saved.
5. For `;`, finish the step: a single-step request returns; a run request continues.
6. For `,`, continue within the current step.

If multiple stop reasons arise from the same command, report error/mismatch before breakpoint, then `!`, then step boundary. Structural loop transitions are transparent to stepping, but retain enough state to resume at a body boundary exactly once. A syntactically complete file may end after any valid terminator or closed block. EOF completes execution only after all finite loops have finished and output finalization has succeeded.

**[Decision]** Results are distinct: `CompletedUnchecked` (no comparison requested), `Passed` (complete comparison), `FailedComparison`, `FailedError`, `Suspended`, `Cancelled`, and `LimitExceeded`. Registering a compare stream without comparing any generated lines does not constitute a passing test; return an error or an explicitly incomplete comparison result. A cancellation/command budget is a caller-configured execution limit; exceeding it must not be reported as success. Check cancellation inside infinite loops and before/after backend actions.

**[Book: p. 403]** If the hardware simulator loads no user script, its default script repeatedly executes `tick, tock;`. **[Decision]** Expose this as an explicit interactive convenience; a command-line API supplied a nonexistent script must fail rather than silently run the default forever. Rewind resets script execution; for reproducible batch reruns create a fresh session and replay setup. Do not confuse a script rewind with asserting a circuit's `reset` input.

## 7. Output formatting

### 7.1 Schemas and fields

**[Book: pp. 398–399]** An output item has the compact form `variable%FpadL.len.padR`. `F` is `B`, `X`, `D`, or `S`; omitted formatting means `%B1.1.1`. The integers specify left spaces, value-column width, and right spaces. A field ends with `|`; output-list writes a header containing variable names.

**[Decision + Reference]** Require `padL >= 0`, `padR >= 0`, and `len >= 1`; impose documented allocation limits on their sum. Start every header/data row with `|`, then append each formatted field plus `|`, then one line ending. Do not add a Markdown separator row, an extra blank row, or quotes. Write LF by default; the comparator removes the line ending only.

**[Reference]** Header algorithm, for each item with `W = padL + len + padR`:

1. Use the script's variable spelling; truncate it to its first `W` characters if longer.
2. Center that name in all `W` columns, with `floor((W-nameLength)/2)` spaces on the left and the remainder on the right.

Header alignment does not use the numeric data alignment or merely the middle `len` columns. **[Decision]** Both profiles use this algorithm; preserve source spelling even when a case-folded lookup resolves a name.

### 7.2 Data conversion and width

The book does not fully specify padding within `len`, overflow clipping, hex letter case, or header alignment. The following **[Decision]** adopts the inspected reference's output rules for the 1–16-bit Hack backend:

- `%B`: render the numeric value as 16 binary digits, zero-padded and masked to 16 bits.
- `%X`: render the numeric value as four hexadecimal digits, zero-padded and masked to 16 bits, using lowercase `a`–`f`. The book's illustration uses uppercase `FFFF`; do not mistake this for a verified casing requirement of every simulator.
- `%D`: render the scalar's decimal value, including a negative sign when applicable.
- `%S`: use the variable's textual representation without numeric conversion. For `time`, use `t` or `t+`; a literal `%S"..."` is not an output-list variable.
- If the rendered text is longer than `len`, keep its **rightmost** `len` characters. This applies even to decimal and string fields; it can hide a minus sign. It is output clipping, not assignment overflow.
- Right-align numeric text in `len` using spaces; left-align string text. Then add `padL` and `padR` spaces. Binary/hex values narrower than 16/four columns are clipped from those representations; widths larger than those representations add spaces, not more zero digits.

Thus `%B0.1.0` prints the low bit of the 16-digit representation, whereas `%D0.1.0` applied to -1 prints `1`. Tests should choose adequate widths; clipping does not make the underlying values equal.

**[Reference caveat]** The legacy time accessor returns a trailing blank on integral times (`"1 "`) and `+` on half-cycles (`"0+"`). Normal `%S` output with adequate width masks that difference through padding. `book-hardware` returns `"1"` canonically. Very narrow string fields and string conditions can expose the difference.

### 7.3 Exact example

**[Decision example]** For a loaded chip with one-bit input `a` and output `out`:

```tst
output-file Gate.out,
output-list a%B1.1.1 out%B1.1.1;
```

This emits the header below; a later `output;` with `a=0`, `out=1` emits its second line:

```text
| a |out|
| 0 | 1 |
```

Each field is three columns between dividers. The header and data row are intentionally spaced differently.

## 8. `.cmp` and `.out` comparison

**[Book: pp. 399–400]** `compare-to` enables line-by-line checking; a differing generated line stops the script. Output is written before the comparison. **[Reference]** The same writing/comparing function handles the header generated by `output-list`, so the first comparison normally checks the header, not the first data row.

**[Decision + Reference]** Use an explicit comparator, not a numeric table parser:

1. Read exactly one expected physical line for each generated header or data line after comparison is enabled. Do not skip blank lines or comments.
2. Strip LF/CRLF line terminators only; preserve spaces, case, dividers, and all other characters.
3. Require equal line lengths.
4. At each position, require equal characters, except that an expected `*` matches any **one** actual character. It is not a variable-length glob and does not consume a whole field.
5. Record a mismatch at its one-based expected physical line and, where possible, its first differing column. Retain the actual failing line in `.out` and stop before the next command.

Wildcard matching is **[Reference]**, not an explicit Appendix 3 rule. Do not use HDL constants, regexes, numeric equivalence, whitespace normalization, or case folding for this comparison. The standalone TextComparer utility described on the official software page ignores whitespace; that is a different comparison operation.

**[Decision]** Early expected EOF is a comparison failure with a diagnostic, not an exception or null dereference. In `book-hardware`, successful finalization also requires expected EOF; leftover expected lines fail as missing actual output, including leftover blank lines. The inspected legacy controller does **not** check for leftover expected lines at normal completion. `legacy-java` reproduces that prefix acceptance, but must report that mode in the result. Robust EOF handling remains required in both profiles.

Changing `compare-to` starts a new expected cursor. Changing only `output-file` resets the actual file's physical line count but does not implicitly rewind the expected stream. A late `compare-to` starts at expected line 1 and does not retroactively check earlier rows. Track actual and expected counters separately. A repeated `output-list` emits another header and consumes another expected line. Close/flush streams at terminal completion and preserve a structured result for any failure.

## 9. Diagnostics and error behavior

Appendix 3 specifies stops for comparison failure and debugging, but does not standardize all diagnostics or exit codes. The following is **[Decision]**:

| Category | Examples | Required response |
|---|---|---|
| Lexical / syntax | Unterminated quote/comment, missing terminator, invalid relation, nested/empty loop | Reject script before execution; report source span |
| Command / capability | Wrong arity, `vmstep` on hardware backend, unknown method | Fail with command and backend identified |
| Resolution / compilation | Missing HDL, malformed local stub, bad child pin, dependency cycle | Preserve HDL diagnostic plus script `load` location and resolution trace |
| Variable / type | Unknown/read-only variable, width overflow, mixed condition types, invalid index | Fail the command before mutation |
| State / phase | No loaded chip, illegal tick/tock sequence, invalid schema after reload | Fail before advancing the cursor |
| I/O | Missing `.cmp`, unreadable ROM, unwritable `.out` | Identify operation and path; retain prior output |
| Comparison | Character mismatch, early expected EOF, unconsumed expected lines in complete mode | `FailedComparison`, with line/column and expected/actual text |
| Control | `!`, breakpoint, cancellation, command limit | Return the distinct status from §6; never report pass |

Diagnostics must contain a stable machine-readable code, human explanation, filename, one-based line/column, source span where available, and the backend/profile. Runtime errors include the current command and loaded chip identity. Resource limits are explicit configuration and have their own code. Exact legacy English messages and off-by-one line-number quirks are not contractual.

Parse errors must not create/truncate output files. Validate a value, method program, or schema completely before applying that operation. This does not make a whole script transactional: prior successful commands and output remain observable after a later failure. An output row is assembled in memory before writing; an I/O failure may still leave a partial OS write and must be reported honestly. Pauses preserve streams/cursors for resume; a terminal failure closes them.

## 10. CPU and VM dialect boundaries

**[Book: A3.3–A3.4, pp. 404–409]** Shared control, setup, output, and comparison commands are reused, but the execution backend changes:

| Capability | Hardware | CPU emulator | VM emulator |
|---|---|---|---|
| Top-level `load` | Required `.hdl` basename | `.asm` or `.hack`; assemble `.asm` on load | Optional `.vm` filename or folder; omission means current folder |
| `eval` | Combinational evaluation | Explicitly not applicable | No hardware `eval` capability in this contract; see caveat below |
| `tick`, `tock` | Two hardware half-cycles | Replaced by `ticktock` | Explicitly not applicable |
| `ticktock` | Unsupported baseline command | One machine-instruction cycle | Unsupported |
| `vmstep` | Unsupported | Unsupported | Execute one VM command and advance |
| Built-in chip methods | Registered chip APIs | Explicitly not applicable | Not a VM capability in this contract |

**[Book]** CPU scripting can access architectural registers and RAM. VM scripting can access segment contents and base pointers; VM load starts at `Sys.init` if present, otherwise at the first loaded VM command. These details belong to those backends rather than the HDL compiler.

**Caveat:** A3.4 says it inherits A3.2 except for listed changes, but does not explicitly reconcile hardware-only `eval` and built-in-method commands with the VM. **[Decision]** Reject them in a VM backend unless a separately documented capability gives them meaning. Do not infer functioning hardware operations from that omission. A hardware-only implementation must parse/report other-dialect commands as unsupported, not simulate them as empty actions or treat `ticktock` as a baseline hardware keyword.

The figure tables in A3.3/A3.4 are not a complete VM/CPU backend implementation contract. Supporting those full backends requires separate architecture/VM specifications; this document's required executable scope is hardware testing.

## 11. Examples and acceptance tests

### 11.1 Combinational test

This original example requires an implemented `Not.hdl` or an explicitly available built-in, and an expected file with the exact lines shown.

```tst
load Not.hdl,
output-file Not.out,
compare-to Not.cmp,
output-list in%B1.1.1 out%B1.1.1;

set in 0, eval, output;
set in 1, eval, output;
```

`Not.cmp`:

```text
|in |out|
| 0 | 1 |
| 1 | 0 |
```

### 11.2 Sequential observation

```tst
load DFF.hdl,
output-file DFF.out,
output-list time%S1.4.1 in%B1.1.1 out%B1.1.1;

set in 1, eval, output;
tick, output;
tock, output;
```

**[Decision initial state + Book clock rule]** With zero initial DFF state, the three observations have times `0`, `0+`, `1` and outputs `0`, `0`, `1`. No `.cmp` is requested, so completion is unchecked. A RAM-address test must additionally show asynchronous reads at fixed time; DFF behavior alone does not verify mixed-path timing.

### 11.3 Minimum conformance suite

An implementing agent MUST provide fixtures covering:

1. Comment forms, CRLF, multiline setup, literal punctuation, invalid terminators, exact file-name case, and book-profile command case folding.
2. Whitespace-separated output-list entries and rejection of comma-separated entries interpreted as one list.
3. All three terminators, single-step versus run, resume after `!`, and a pause inside a repeat without replaying its last command.
4. Finite repeat, omitted repeat count, rejected zero/negative count, rejected empty/nested blocks, while initially false, and live-condition exit.
5. Binary/hex/decimal equivalence at 16 bits; `%XFF` zero extension; one-bit inputs; read-only writes; malformed digits; narrow overflow and positive decimal 65535 rejection.
6. Loaded top-level/internal pins, scalar built-in state, first/last/invalid RAM/Screen indices, missing and ambiguous built-in instances, and ROM load failure without partial replacement.
7. DFF half-cycles, parallel DFF capture, mixed RAM address/write timing, explicit Register debug-write behavior, and illegal phase sequences.
8. Exact header centering, default format, lower-case hex, negative decimal, numeric/string alignment, rightmost clipping, and repeated output-list headers.
9. Header comparison, exact spaces/case, one-character `*`, wildcard length mismatch, early/extra expected lines, mismatch row retained, and output without compare.
10. Breakpoint checks after a comma command versus step end, latch/rearm behavior, echo outside `.out`, command limits, cancellation, and no-output comparison incompleteness.
11. Local HDL overriding built-ins, a local malformed/stub file not falling back, missing files, and working-directory-independent script invocation.
12. Unsupported CPU/VM commands, capability manifests, and a fresh identical rerun producing the same output/result.

Reference comparisons must pin the source/distribution version and profile. Record tested behavior separately from source-inspected behavior; neither a spec example nor a source read is an executed compatibility test.

## 12. Compatibility ledger and source map

| Issue | `book-hardware` contract | Inspected legacy Java behavior |
|---|---|---|
| Case | Case-insensitive commands/formats, exact-then-unique-folded variable lookup | Exact-case controller keywords, simulator commands, variable names, format letters |
| Loops | Nonempty, nonnested; explicit count >= 1 | Same restrictions; book is silent on these edge cases |
| Pin writes | Input only; calculated pins read-only | Same permissions, but permissive negative-width edge case |
| Breakpoints | Check after each subsequent simple command | Check at simulation-step boundary |
| Output | Reference formatting, including lowercase hex/clipping | Same algorithm; book illustration differs in hex case |
| Time scalar | Canonical `t` / `t+` text | Integral time includes trailing space |
| Comparison | Exact columns, expected `*`, complete expected EOF required | Exact columns and expected `*`; no trailing-expected-line check |
| `Register[]` | Supply documented accessor, pending write timing | Plain Register is not exposed via GUI-only lookup |
| Multiple same-name built-ins | Ambiguity error | First matching GUI-capable built-in |
| ROM `set` | Read-only; initialize through method | Direct indexed setter is available |
| RAM/Screen reads after tick | Pin reads use committed storage until tock; inspection may expose pending storage | A dirty asynchronous read can observe backing storage changed at tick |
| Errors / pauses | Structured, headless-safe outcomes | Includes GUI dependencies and rough edge cases; not a portable error contract |

**[Decision]** A `legacy-java` adapter applies the right column where stated. It need not reproduce crashes, unchecked exceptions, GUI bugs, or unsafe I/O. All remaining deliberate deviations must be named in the capability manifest; merely enabling a flag is not evidence of complete legacy emulation.

Primary source locations:

| Subject | Second-edition PDF |
|---|---|
| Files, current folder, comments, script case, controls | A3.1, pp. 393–395 |
| Script example, literal formats, time, built-in table | A3.2, pp. 396–397, figures A3.1–A3.2 |
| Separators and setup commands | A3.2, pp. 398–399 |
| Simulation commands, control flow, state/method access | A3.2, pp. 399–403 |
| CPU and VM differences | A3.3–A3.4, pp. 404–409 |
| Clocked paths, feedback, unconnected pins | A2.4/A2.6, pp. 381–384, 387–391 |

Supplementary primary implementation sources, all pinned to the inspected commit:

- [Script.java: parsing, list defaults, loop restrictions](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/HackPackageSource/Hack/Controller/Script.java)
- [ScriptTokenizer.java: lexical case behavior](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/HackPackageSource/Hack/Controller/ScriptTokenizer.java)
- [HackController.java: output, wildcard comparison, breakpoint checks](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/HackPackageSource/Hack/Controller/HackController.java)
- [HardwareSimulator.java: pin permissions, phase errors, built-in lookup, time](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/SimulatorsPackageSource/Hack/HardwareSimulator/HardwareSimulator.java)
- [Conversions.java: numeric representations](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/HackPackageSource/Hack/Utilities/Conversions.java)
- [BuiltInChipsSource](https://github.com/nand2tetris/nand2tetris_simulator/tree/327b7e4337b31efb815e1c1e3913c0efd299bcf2/BuiltInChipsSource) and [built-in HDL interfaces](https://github.com/nand2tetris/nand2tetris_simulator/tree/327b7e4337b31efb815e1c1e3913c0efd299bcf2/InstallDir/builtInChips): state accessors, PC width, Screen size, debug setters.

This is an independently organized engineering specification, not an official publisher standard. Textbook facts are paraphrased; unresolved implementation choices are identified rather than attributed to the textbook.
