# Nand to Tetris HDL Language Specification

**Status:** Implementation-oriented specification, version 0.1  
**Target:** The *Nand to Tetris*, second-edition `.hdl` dialect  
**Primary reference:** Nisan & Schocken, *The Elements of Computing Systems*, 2nd ed., Appendix 2, pp. 345 onward in the printed book (PDF pp. 373–390); especially §§ A2.1–A2.4 and A2.6.

> **Conformance note:** The textbook provides a language description, not a fully formal grammar or a complete account of every error condition. Sections marked **[Specified]** restate its documented behaviour in implementation-ready form. **[Derived]** denotes a logical consequence. **[Implementation policy]** identifies choices that a compatible parser/interpreter must settle and ideally validate against the reference Hardware Simulator. Do not assume that a policy is mandated by the textbook.

## 1. Scope and computational model

**[Specified]** An HDL source file defines one *chip*, whose public interface comprises named, fixed-width **input** and **output** pins. Its implementation is either:

- **Composite:** an unordered set of instances of other chips, with connections between instance pins and signals in the containing chip; or
- **Built-in:** an external implementation selected by a `BUILTIN` declaration.

An HDL definition specifies a circuit, not an algorithm. Evaluation order is therefore unrelated to source statement order. Composite chips may contain further composite chips, until primitive/built-in implementations are reached.

The extension for source files is `.hdl`. If the declaration is `CHIP Foo`, the source file is named `Foo.hdl` **[Specified]**.

This specification does not cover `.tst` (test scripts), `.cmp` (expected results), machine-language files or any higher-level HDL such as Verilog.

## 2. Lexical conventions

### 2.1 Character set and case

**[Specified]** The language is case-sensitive. The reserved words are written in uppercase (`CHIP`, `IN`, `OUT`, `PARTS`, `BUILTIN`, `CLOCKED`), except for the Boolean constants `true` and `false`, which are lowercase.

**[Specified]** Names consist of letters and digits and do not begin with a digit. Case is significant. The book mentions that some simulator versions accept hyphens and others do not.

**[Implementation policy]** For portable projects, take identifiers to match `[A-Za-z][A-Za-z0-9]*` and reject hyphens in strict mode. Treat the keywords and Boolean literals as reserved rather than valid user-defined names.

### 2.2 Whitespace and comments

**[Specified]** Whitespace is insignificant between tokens. Three comment forms are supported:

```hdl
// line comment
/* block comment */
/** documentation comment */
```

Documentation comments have the same parsing effect as ordinary block comments, although tooling may attach them to declarations. The reference does not specify nested block comments; **[Implementation policy]** treat the first `*/` as the end of a block comment and reject an unterminated comment.

### 2.3 Punctuation and integer literals

Structural punctuation: `{`, `}`, `(`, `)`, `[`, `]`, `,`, `;`, `:`, `=`, `..`. The double dot is a *range separator*, not two independent member-access operators.

Decimal non-negative integers are used for widths and slice indices. **[Implementation policy]** Tokenise these as `[0-9]+`, with overflow and bound checks during semantic analysis.

## 3. Concrete grammar

The grammar below uses EBNF. `{ X }` means zero or more repetitions, `[ X ]` means optional, and `|` means alternatives. `IDENT` and `INTEGER` are tokens as defined above. The grammar is a **formalisation**, not an official grammar copied from the book.

```ebnf
file             = chip_declaration, EOF ;
chip_declaration = "CHIP", IDENT, "{", input_section,
                   output_section, implementation, "}" ;

input_section    = "IN", pin_declaration,
                   { ",", pin_declaration }, ";" ;
output_section   = "OUT", pin_declaration,
                   { ",", pin_declaration }, ";" ;

pin_declaration  = IDENT, [ "[", INTEGER, "]" ] ;

implementation   = composite_implementation
                 | builtin_implementation ;

composite_implementation
                 = "PARTS", ":", { part_instance } ;
part_instance    = IDENT, "(", [ binding, { ",", binding } ],
                   ")", ";" ;
binding          = pin_reference, "=", source ;

pin_reference    = IDENT, [ selector ] ;
source           = pin_reference | "true" | "false" ;
selector         = "[", INTEGER, [ "..", INTEGER ], "]" ;

builtin_implementation
                 = "BUILTIN", IDENT, ";",
                   [ "CLOCKED", [ IDENT, { ",", IDENT } ], ";" ] ;
```

**Grammar caveats:**

1. **[Specified]** `IN` and `OUT` are normal chip-interface sections, followed by a `PARTS:` body or by `BUILTIN ...;`. The grammar models that canonical form. Alternative omissions or section orderings are not documented in the book.
2. **[Specified]** Built-in chips can declare clocked pins with `CLOCKED`, including an empty list. The book presents `CLOCKED` for built-in implementations, not composite ones.
3. **[Implementation policy]** An empty `PARTS:` section is accepted syntactically (useful for supplied stubs), although it may be a non-functional implementation. Whether empty `IN` or `OUT` lists are accepted by the original simulator should be tested if needed.
4. **[Implementation policy]** Syntax permits selectors on either side of `=`; semantic constraints below determine whether a selector is legal.

## 4. Interface declarations

### 4.1 Pin kinds

**[Specified]** There are three kinds of signals within a composite chip:

- **Top-level input:** externally driven; declared in `IN`.
- **Top-level output:** externally observable; declared in `OUT`.
- **Internal signal:** automatically introduced by a connection inside `PARTS:`; not explicitly declared.

Pin widths are constant integers. `name` means width 1; `name[N]` in an `IN` or `OUT` declaration means width **N**, not index N.

**[Derived]** A declaration width must be positive; `[0]` could not denote a non-empty pin. This is a useful validation requirement even though the book does not explicitly specify an error message for it.

### 4.2 Width versus index

The same brackets have two distinct meanings:

```hdl
IN address[15];   // width 15 (indices 0..14)

// Inside a part instance:
SomeChip(in=address[0..7]); // eight selected bits
```

**[Specified]** Bus bit 0 is the least-significant (rightmost) bit. A selector `[i]` extracts one bit; `[i..j]` selects the inclusive interval i through j. Its width is `j - i + 1`.

**[Implementation policy]** Reject selections with `i > j`, out-of-range indices, or selections outside the declared width. These are natural requirements for correct wiring; the book does not standardise the exact diagnostic behaviour.

## 5. Instances and connections

### 5.1 Instances

Each statement in `PARTS:` constructs an instance of the named chip type:

```hdl
Nand(a=a, b=b, out=temporary);
Not(in=temporary, out=out);
```

**[Specified]** `Nand` and `Not` above are **chip type names**; the names on the left of `=` (`a`, `b`, `out`, `in`) are formal pins of *that particular instance*. The names on the right are signals in the **containing** chip, or the literals `true` and `false`.

Consequently, `a=a` is not an assignment to itself; it connects the instance's formal pin `a` to the containing chip's signal `a`.

Instances do not have explicit user-assigned instance names. **[Implementation policy]** Assign stable internal instance IDs in source order (e.g. zero-based numbers) for diagnostics, state storage and LSP navigation; the order must not affect circuit results.

### 5.2 Directionality

**[Specified]** The direction of a connection depends on the *formal pin's declaration*, not on which side of `=` is written:

- If the formal pin is an **input**, values flow from the right-hand signal or constant **into** the instance.
- If the formal pin is an **output**, values flow from the instance **into** the right-hand signal.

The connection operator `=` therefore declares a wire rather than a mutable assignment.

**[Derived]** A constant is not a valid destination for a part's output pin. Likewise, a top-level input cannot meaningfully be driven by a part output, and a top-level output is normally supplied by a part output. Check such cases during elaboration.

### 5.3 Fan-in, fan-out and defaults

**[Specified]** Each pin may have a single driver (**fan-in 1**) but may feed any number of destinations (**unlimited fan-out**). The same internal signal can therefore feed multiple chip inputs.

**[Specified]** The official simulator defaults unconnected input and output pins to `false` (zero), rather than considering disconnection a fatal error. This also means misspelt internal signals may silently result in incorrect circuits.

**[Implementation policy]** Preserve reference-compatible zero defaults in compatibility mode; additionally report unused, undriven or unconsumed signals as *warnings* for editor support. Reject conflicting multiple drivers unless testing proves the reference simulator has a narrower or different policy.

## 6. Buses and signal slicing

### 6.1 Signal widths

**[Specified]** Top-level pin widths are explicit. An internal signal's width is inferred from the output of a chip-part connected to it. For example:

```hdl
// If SomeChip's 'out' is a 16-bit output:
SomeChip(out=wideSignal);     // wideSignal has width 16
SomeChip(out[0..7]=lowBits);  // lowBits has width 8
SomeChip(out[8]=oneBit);     // oneBit has width 1
```

**[Specified]** **Internal signals cannot be indexed or sliced.** Slicing is allowed on the containing chip's declared input/output pins or on the formal input/output pins of a chip-part. The restriction is deliberately non-obvious and important for conformance.

```hdl
SomeChip(out=internalBus);
OtherChip(in=internalBus[0..7]); // INVALID: internalBus is internal
```

To obtain a sub-bus, select it directly from a chip-part pin while also wiring the full bus as required, using multiple bindings.

### 6.2 Bus connections

**[Derived]** For a bitwise connection, the selected widths on the two sides must match. For example, connecting an 8-bit destination to a 16-bit signal requires selecting an 8-bit interval. This should be validated after loading the callee's interface.

### 6.3 Multiple bindings to one formal pin

**[Specified]** A formal pin can occur in more than one binding where different slices are involved. This supports assembling input buses from separately connected regions, and splitting outputs into separate internal signals.

```hdl
// Assumes SomeChip declares an 8-bit input named in.
SomeChip(in[0..2]=true, in[6..7]=true, out=result);
```

**[Specified]** The remaining input bits are zero, giving the eight-bit value `11000111` (bit 7 at the left). `true` and `false` are single-bit Boolean constants which can fill a selected input slice.

**[Implementation policy]** Track driven/connected bit intervals, not just whole-pin names. Treat overlap between conflicting input bindings or multiple output drivers as an error; precise duplicate-binding behaviour is not formally defined in the book.

## 7. Boolean constants

**[Specified]** `true` means binary one and `false` means binary zero. These may be wired to single-bit inputs or used to fill a selected multi-bit input (including an entire bus).

```hdl
And(a=true, b=flag, out=result);
Mux16(a=false, b=data, sel=choose, out=result16);
```

`true` and `false` are *values*, not user-defined signal identifiers. There is no general numeric literal syntax for connections in the documented language (`42`, `0b1010` etc. are not valid source expressions).

## 8. Built-in definitions

**[Specified]** A built-in chip declares its public pin interface in HDL but delegates its implementation to simulator-supplied executable code. Example:

```hdl
CHIP And16 {
    IN a[16], b[16];
    OUT out[16];
    BUILTIN And16;
}
```

In the original Java simulator, the built-in name resolves to a Java implementation. **[Implementation policy]** A Go interpreter should resolve the same symbolic name through a built-in registry, not attempt to execute Java classes.

Loading a subchip follows these described reference simulator rules **[Specified]**:

1. Look for `ChipName.hdl` in the current project directory.
2. If an `.hdl` definition is present, load and use it (including a stub, which may fail to simulate).
3. If absent, use a simulator-provided built-in implementation where available.
4. If neither is available, chip resolution fails.

This differs from always preferring built-ins to source files. Note that CPU and Computer are not among the simulator's automatically supplied Hack built-in implementations according to the book.

## 9. Evaluation and clock semantics

### 9.1 Combinational evaluation

**[Specified]** Combinational chips are time-independent: their outputs depend on the current inputs and connected combinational logic. The simulator should evaluate dependencies rather than step through `PARTS` statements in written order.

**[Derived]** A sensible evaluator elaborates instances into a directed netlist, derives dependency ordering and recomputes affected values when inputs change. This evaluation algorithm is not a mandated part of the language.

### 9.2 Sequential evaluation

**[Specified]** The simulator exposes a two-phase clock, `tick` then `tock`. During `tick`, clocked chip parts determine their next state from current inputs; at `tock`, updated state becomes externally visible. Combinational logic does not require clock steps.

**[Specified]** A built-in chip may declare clock sensitivity:

```hdl
CHIP DFF {
    IN in;
    OUT out;
    BUILTIN DFF;
    CLOCKED in;
}
```

A `CLOCKED` declaration identifies clock-sensitive formal pins. The list may be empty. Composite chips acquire sequential behaviour transitively from sequential descendants; their own `.hdl` source does not add an explicit `CLOCKED` clause.

**[Implementation policy]** Implement a global two-stage update model: capture next state from the pre-commit state at `tick`, then commit at `tock`; settle combinational outputs after each relevant change. This prevents an accidentally sequential order across otherwise parallel DFF instances. Keep internal primitive-specific state behind the built-in implementation.

### 9.3 Feedback loops

**[Specified]** Feedback is valid when every cyclic dependency is interrupted by an appropriate clocked pin; a purely combinational cycle is invalid. This is determined recursively through the chip hierarchy.

```hdl
// Invalid: instantaneous feedback.
Not(in=loop, out=loop);

// Valid in principle: feedback passes through DFF's clocked input.
DFF(in=loop, out=loop);
```

**[Implementation policy]** Build a pin-level dependency graph with clocked boundaries. Reject cyclic combinational dependency strongly connected components; permit cycles that cross a clocked boundary. Merely checking whether a chip *contains* a DFF is insufficient for chips with mixed clocked and unclocked paths.

## 10. Static semantic validation

The following is a recommended validation matrix, separating explicit rules from implementation policy:

| Check | Severity | Basis |
|---|---|---|
| Invalid tokens, malformed syntax | Error | Grammar |
| CHIP name differs from filename | Error | Specified |
| Unknown child chip type | Error | Derived from resolution |
| Unknown child formal pin | Error | Derived |
| Bus selector outside width | Error | Derived |
| Slicing internal signal | Error | Specified |
| Nonmatching bus widths | Error | Derived |
| Multiple conflicting drivers | Error | Specified fan-in rule |
| Purely combinational cycle | Error | Specified |
| Assignment to constant or top-level input | Error | Derived |
| Unconnected input/output | Warning | Reference allows and defaults to zero |
| Never-read internal signal | Warning | Tooling enhancement |
| Empty supplied stub implementation | Warning | Tooling enhancement |
| Duplicated top-level pin names | Error | Implementation policy |
| Self-recursive chip dependency | Error | Implementation policy |

Actual simulator diagnostic strings, treatment of repeated identical bindings, and specific missing-pin edge cases are **not** fully specified in the book and warrant compatibility tests.

## 11. Example: composite chip

```hdl
/** Computes whether three one-bit inputs are all equal. */
CHIP Eq3 {
    IN a, b, c;
    OUT out;

    PARTS:
    Xor(a=a, b=b, out=differentAB);
    Xor(a=b, b=c, out=differentBC);
    Or(a=differentAB, b=differentBC, out=anyDifferent);
    Not(in=anyDifferent, out=out);
}
```

This example demonstrates implicit internal wires, chip-instance formal pin names, fan-out and the order-independent network.

## 12. Suggested parser and interpreter architecture (non-normative)

For a Go implementation, separate the following stages:

1. **Lexer:** preserve token ranges, comments and documentation trivia for editor functions.
2. **Parser:** produce a lossless or location-aware AST (chip, declarations, implementation, instances, bindings, selectors).
3. **Symbol resolution:** load child interfaces and select local `.hdl` files or built-ins. Detect chip dependency cycles.
4. **Elaboration and type-checking:** resolve each binding's direction, infer internal wire widths, validate slices and drivers, and expand bus connections to bit ranges.
5. **Netlist construction:** instantiate chip types with individual runtime state; derive combinational dependencies and clock boundaries.
6. **Simulator:** evaluate settled combinational values and the global `tick`/`tock` state update.
7. **LSP:** reuse parser, symbol tables and elaborator diagnostics for diagnostics, completions, hover info, go-to-definition and references.

Keep syntax trees independent of runtime logic: a parser should never have to simulate the chip to answer LSP queries.

### Representative AST shapes

```text
ChipDecl
  name: Identifier
  inputs: []PinDecl
  outputs: []PinDecl
  implementation: CompositeDecl | BuiltinDecl

CompositeDecl
  parts: []PartInstance

PartInstance
  chipType: Identifier
  bindings: []Binding

Binding
  formalPin: PinReference
  actual: PinReference | BooleanLiteral

PinReference
  name: Identifier
  selector: None | Bit(index) | Range(start, end)
```

The parser should not assume that the right-hand identifier is internal: whether a name denotes a top-level input, output, or implicit internal signal is a **semantic** question.

## 13. Compatibility test corpus

At minimum, test the following against both your implementation and the original simulator:

- Normal composite chip using Nand, Not, Xor and Or.
- Source part reordering with identical results.
- Same identifier on the left and right of a binding.
- One internal wire feeding multiple chip inputs.
- Width-16 declared bus, single-bit selection and inclusive ranges.
- Multiple bindings supplying disjoint ranges of one input pin.
- Full-bus `true`/`false` binding and partially populated buses.
- Attempted indexing of an internal signal.
- Misspelt/undriven signals (zero default versus diagnostics).
- Built-in resolution versus locally present HDL and stub files.
- Direct combinational feedback versus feedback through DFF.
- `tick`/`tock` state visibility and multiple DFFs updating together.
- Invalid width, out-of-range selection, conflicting drivers and duplicate formals.

## 14. Known specification boundaries

The textbook does **not** provide a normative formal grammar or comprehensive diagnostic/error specification. In particular, precise identifier extensions, duplicate connections, recovery from invalid declarations, exact built-in API integration and some ambiguous wiring edge cases require either an explicit implementation policy or direct experiments with the canonical Hardware Simulator.

For a toolchain intended to be drop-in compatible with existing Nand to Tetris projects, favour observed simulator behaviour in compatibility mode, whilst enabling additional lint checks separately. Keep these distinctions visible in documentation and diagnostics.

---

**Source attribution:** Based on the language description in Appendix 2 of *The Elements of Computing Systems*, 2nd edition, by Noam Nisan and Shimon Schocken (MIT Press, 2021). This is an independently organised engineering specification, not an official or publisher-endorsed standard.
