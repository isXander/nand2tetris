# Nand2Tetris HDL and test integration specification

Version 1.0 · 26 October 2026 · Repository implementation contract

## 1. Read this document with its two siblings

The intended repository layout is:

```text
nand2tetris-spec.md
nand2tetris-hdl-spec.md
nand2tetris-tst-spec.md
```

All links are relative to that same directory:

| File | Responsibility |
|---|---|
| [nand2tetris-hdl-spec.md](nand2tetris-hdl-spec.md) | Existing HDL language specification: chip interfaces, parts, bindings, widths, built-in declarations, clocked paths, and static validation |
| [nand2tetris-tst-spec.md](nand2tetris-tst-spec.md) | Test language: lexical syntax, commands, values, control flow, output formatting, comparison, errors, and backend compatibility boundaries |
| [nand2tetris-spec.md](nand2tetris-spec.md) | This document: resolution, compilation, elaboration, runtime boundaries, timing, source corrections, architecture, and the implementing agent's delivery contract |

The first filename is the **exact basename of the previously created document**. `hdl-spec.md` is only an informal shorthand in the request; there is no additional sibling with that name. Do not create links to it or silently rename the original. The original was located in Downloads and read in full; an unchanged copy accompanies these specs.

### 1.1 Evidence labels and precedence

The primary authority is Nisan and Schocken, *The Elements of Computing Systems*, second edition (MIT Press, 2021), especially Appendix 2 (HDL) and Appendix 3 (Test Description Language). Both appendices and the embedded figures relevant to these specs were checked in the supplied PDF. All PDF page numbers are **one-based file pages**, not printed pagination.

- **[Book]** means the textbook explicitly states the rule.
- **[Derived]** means a consequence of the documented model.
- **[Reference]** means inspected official legacy Java source, pinned to commit `327b7e4337b31efb815e1c1e3913c0efd299bcf2`.
- **[Decision]** means a repository implementation choice, including formal grammars, module boundaries, diagnostics, APIs, reset policies, and algorithms.

The older HDL document's `[Specified]`, `[Derived]`, and `[Implementation policy]` labels correspond to `[Book]`, `[Derived]`, and `[Decision]`; preserve that distinction. The book is descriptive, not a fully formal interoperability standard. The source snapshot is evidence about one reference implementation, not proof of every published simulator's behavior. No reference binary was executed during preparation of these documents.

**[Decision]** Interpret the three documents together. Corrections in §2 override the older HDL document at the identified points; otherwise that document governs HDL syntax and wiring. [tst-spec.md](tst-spec.md) governs scripting and comparison details. This document governs integration and common runtime policy. Source disagreements must be surfaced as named profile differences, not silently called textbook facts. Required scope is the `book-hardware` profile; the `legacy-java` differences are specified compatibility work, not a claim that it is already implemented.

### 1.2 Purpose and non-goals

**[Book: A2.1, pp. 373–376; A3.1, pp. 393–395]** HDL describes a circuit. A test script drives a simulator through scenarios that load and exercise that circuit. The supplied tests check observable behavior against expected results. Passing a finite test suite does not prove a chip correct for all inputs, contexts, or time histories (A2.6, pp. 388–389).

**[Decision]** This contract targets an implementation that a fresh-context coding agent can build and test without guessing hidden architectural assumptions. It does not require a particular programming language, existing repository structure, GUI framework, or CPU/VM implementation. The older HDL spec uses Go as an illustrative implementation language; the module/API examples below remain conceptual. Adapt them to the repository's actual conventions.

## 2. Required corrections and qualifications to the existing HDL spec

Preserve [nand2tetris-hdl-spec.md](nand2tetris-hdl-spec.md) verbatim unless separately asked to revise it. Apply these supplements when implementing it:

1. **Interface sections may be omitted when empty.** The grammar in its §3 requires both an input and output section. **[Reference + Book chip set]** Official `Keyboard.hdl` has only `OUT`; the top-level Computer interface has `IN reset` and no output pins. **[Decision]** Parse optional input and output sections in canonical order, each at most once, with nonempty lists when present. Do not require dummy `IN;` or `OUT;` declarations. An absent side denotes zero pins. This is needed to support the actual supplied chip set.
2. **A containing output is not a reusable internal source in the legacy dialect.** Its §5.2 is not a complete legality matrix. **[Reference]** `CompositeGateClass.addConnection` rejects a gate output connected to a part input. **[Book: A2.6, p. 391]** The documented workaround wires a part output to both a top-level output and a separate internal wire. **[Decision]** Reject use of a containing output as a part-input source; use a separate internal net for reuse. Multiple output bindings may fan out from the same part output; do not mistake this for multiple independent drivers.
3. **Clocking is path-sensitive.** Its §9 correctly calls for clock boundaries, but a global rule that postpones all outputs of any chip containing state would be wrong. **[Book: A2.4, pp. 382–384]** Clocked input/output pins delay specified dependencies; RAM address-to-output paths remain combinational. The empty `CLOCKED;` form declares clock-dependent internal behavior without declaring delayed interface paths.
4. **Source definition recursion and circuit feedback are different.** The registry may recursively compile referenced source chips (§4). **[Decision]** Reject infinite structural inclusion such as `A` containing `B` containing `A`, even if the declarations mention clocked parts. A finite circuit feedback loop through a clocked path is allowed. Signal-cycle validation cannot replace source-inclusion-cycle detection.
5. **Zero defaults do not erase every semantic error.** **[Book: A2.6, p. 389]** Unconnected input/output pins are allowed and default to zero. **[Reference]** A named internal net with no part-output source can separately trigger an error. **[Decision]** Keep the explicit zero-default rule for disconnected declared pins and unused input bits. In the baseline, reject an internal net that is consumed but has no source; distinguish this from a declared unconnected input. Make any more permissive internal-net behavior a named option. Do not promise every misspelt wire always becomes zero in every reference version.
6. **No timing for debug setters follows from HDL alone.** The old document's capture/commit model describes circuit operations. **[Book/Reference disagreement]** Script-visible built-in state setters can behave differently. Implement the per-built-in script API described in [tst-spec.md](tst-spec.md), §4, rather than imposing one timing rule on all setters.

The corrected interface fragment is **[Decision]**:

```ebnf
chip = "CHIP", identifier, "{", [ input_section ], [ output_section ],
       implementation, "}" ;
input_section  = "IN", pin_decl, { ",", pin_decl }, ";" ;
output_section = "OUT", pin_decl, { ",", pin_decl }, ";" ;
```

These are documented errata/qualifications, not an assertion that every section of the original file was already exact or that it has been edited.

## 3. File identity and resolution

### 3.1 File roles

**[Book: A3.1, pp. 393–394]** The `.hdl`, `.tst`, `.out`, and `.cmp` stems need not match, although matching is recommended. A script's `load` determines the tested chip; opening an HDL file earlier does not override a later script load. A `.cmp` is expected text, and an `.out` is generated observation text; neither is HDL or an executable program.

**[Decision]** Track these distinct paths and identities:

```text
SessionContext
  scriptPath       canonical path of the selected .tst
  projectDir       directory established when opening the script
  builtinLibrary   immutable or versioned descriptor/interface library
  sourceProvider   filesystem or editor-overlay reads with version identities
  profile          book-hardware or an explicitly supported compatibility profile
  limits           caller-supplied command, memory, and expansion bounds
```

The process working directory is not semantic state. CLI callers may give an absolute or relative script path; canonicalize it once. In an interactive environment, selecting a new script establishes a new session directory. Selecting only an HDL file establishes an interactive chip context and may use the default clock script, but must not mutate an already running script's directory. This deterministic policy implements the textbook's intended same-folder workflow without depending on whichever GUI pane last opened a file.

### 3.2 Chip lookup

**[Book: A2.3/A2.6, pp. 379–380, 387–388; A3.2, p. 398]** For a referenced chip type `Xxx`, look for `Xxx.hdl` in the current project folder. If no such source exists, use the supplied built-in chip definition when available. A present incomplete stub or erroneous implementation is selected and can fail; its presence is not permission to fall back. The same current-folder/built-in lookup is described for `load Xxx.hdl`. Baseline script `load` requires the `.hdl` extension and forbids a path.

**[Decision]** Resolve in this order:

1. Validate the chip type and derive the exact-case basename `Xxx.hdl`.
2. Read the project-local file through `sourceProvider`. If present, return that source identity; parse/compile it normally.
3. Only on genuine not-found, ask the configured built-in interface library for `Xxx`. That interface may be supplied as HDL or an equivalent immutable descriptor.
4. If neither exists, report `UnknownChip` with the searched locations.

Unreadable, malformed, mismatched-name, or empty local files are errors, not not-found. A local source containing `BUILTIN ImplName;` is still selected as source and delegates only its implementation body. A named built-in declaration resolves `ImplName` in the executable built-in registry directly; it must not re-run lookup for `ImplName.hdl` and recursively rediscover the same declaration.

**[Decision]** All child chip lookups in a session use the same `projectDir`, matching the reference's working-directory lookup. Do not recursively scan other project folders, search adjacent `completed`/`stubs` directories, or switch to a built-in file's physical directory for nested source lookup. Multi-directory import paths would be a separate, explicitly configured extension. Canonicalize source identities for caches, but preserve requested spellings and diagnostic paths. Enforce case-sensitive chip/file identity even on a case-insensitive host filesystem by validating the actual entry spelling. Script command case folding does not change file spelling.

**[Book]** Moving local stubs out of the working folder causes built-in implementations to be used; moving them back causes the local sources to be used. This is a documented development workflow, not an exceptional recovery mechanism.

### 3.3 Companion files and ROM programs

**[Book: A3.2, pp. 398–399, 402]** `output-file` creates output in the current folder; `compare-to` reads expected results there. `ROM32K load Max.hack` loads a program into an already present built-in part. Figure A3.2 also lists `.asm` input for ROM loading.

**[Decision]** Resolve all baseline companion basenames against `projectDir`. Do not change that directory when writing `.out` or opening `.cmp`. Suffix checks, overwrite protection, and method validation are governed by [tst-spec.md](tst-spec.md). ROM program loading must use the built-in method API and an optional assembler service; an `.asm` file is not input to the HDL parser. Runtime circuit behavior must not depend on output file existence or compare-file values.

## 4. Compiler, elaborator, registry, and simulator

### 4.1 Responsibilities

The following architecture is entirely **[Decision]**; the book does not prescribe package boundaries.

| Component | Inputs | Outputs / duties |
|---|---|---|
| HDL lexer/parser | Source text and source identity | Location-aware AST; syntax diagnostics; no chip execution |
| Source/type resolver | Chip name + session resolution context | Selected source/interface identity; lookup trace |
| HDL compiler/elaborator | AST + child type interfaces/definitions | Validated immutable chip definition or construction plan |
| Built-in registry | Symbolic implementation name | Interface/dependency metadata, factory, script-state/method descriptors |
| Runtime circuit builder | Validated definition + built-in factories | Fresh connected circuit instances and independent state |
| Simulator | Circuit instance, input edits, eval/tick/tock | Published pins, pending/committed state, clock events |
| TST parser/runner | Script text/plan + backend | Ordered commands, loops, stepping, suspensions, structured result |
| Formatter/comparator | Typed observations + schema + expected stream | Exact output rows, comparison events/diagnostics |
| Application/session | Paths, source provider, profile, limits | Composition and lifecycle of the components |
| Editor/LSP | Source snapshots + analysis services | Diagnostics/navigation without running tests |

“Compiler” can mean the whole transformation from source to a runtime construction plan. “Elaboration” is the part that resolves chip types, creates instance structure, checks bindings, and constructs connections. They need not be separate executables or passes; the distinction identifies responsibilities. Elaboration is not the runtime act of computing the chip's output on test inputs.

### 4.2 Recursive resolution is expected

**[Book: A2.1, p. 375]** Composite chip parts recursively contain lower-level chips until externally implemented primitives are reached. **[Derived]** Loading a source child can require invoking the HDL compiler again for that child and its descendants.

**[Decision]** A registry/resolver that returns *compiled chip definitions* may therefore call back into the HDL compiler on a cache miss. This is a valid control-flow recursion and should be explicit. A package dependency cycle is not required. For example:

```text
CompileService.compile(root, context)
  parse root
  for each part type:
    DefinitionLoader.load(type, context)
      select local source or built-in interface
      source -> CompileService.compile(child, same context)
      built-in -> obtain validated descriptor/factory reference
  validate bindings and dependency graph
  return immutable Definition
```

Place `CompileService` and its recursive loader in one compilation layer, or inject a `DefinitionLoader` callback/interface at the application boundary. The runtime core accepts prepared definitions and factories. It must not import the HDL parser merely to create or simulate an instance. Conversely, the parser must not import the runtime to decide what a token means. Both arrangements preserve recursion without introducing cyclic module imports.

A lower-level resolver can alternatively return source identities rather than compiled definitions; then the compiler owns recursive calls itself. Choose one owner of compilation and caching. Do not create two loaders with competing caches or fallback rules.

### 4.3 Compilation invariants

**[Decision]** The compiler/elaborator MUST:

1. Validate declaration/file identity, duplicate names, supported widths, and implementation kind. Honor optional empty-side interface sections (§2).
2. Resolve every child interface before interpreting its bindings. Left sides name that child's formal pins; right sides name the containing chip's signals or Boolean constants. Determine direction from the formal declaration, never from the spelling of `=`.
3. Create internal-net symbols consistently and infer their widths from resolved connections, allowing forward use before the driver's part statement. Source order must not change circuit behavior. Reject inconsistent width constraints and consumed internal nets without a source.
4. Validate declared bus selections, inclusive slice widths, and bit-interval drivers. Reject sub-busing an internal net. Permit one driver to fan out, including multiple aliases of a part output; reject independent conflicting drivers. Reject driving constants or containing inputs and reusing containing outputs as internal sources.
5. Fill disconnected declared pins/unbound input bits with zero as required by the book; report useful warnings separately. Do not turn the documented defaults into mandatory fatal errors under an undisclosed “strict” mode.
6. Detect structural source-inclusion cycles with a loading stack before recursive expansion can overflow. Report the chain and source spans. Memoization must not mistake a partially compiled definition for a completed one.
7. Compute combinational dependency information through the hierarchy, preserving clocked interface paths, and reject instantaneous feedback (§6). A single Boolean “contains a DFF” is insufficient for mixed-path validation.
8. Produce either a complete usable definition or explicit diagnostics. Recovery ASTs may support editor work but must not be silently executable.

**[Decision]** A local supplied stub without executable logic is a load/compile failure in the baseline runner, never an automatic built-in substitution. An editor may still retain its interface and diagnostics. Allowing an intentionally empty constant-zero chip is a separately documented extension; it must not hide incomplete project stubs.

### 4.4 Definitions, instances, and cache identity

**[Decision]** Cache immutable definitions, never live sequential instances. Two occurrences of `Register` share code/interface metadata but own separate state. Two runs of the same test receive fresh instances and streams.

A compiled-definition cache key must include:

- Canonical selected source identity and content version/hash.
- Resolution context/project directory and profile/compiler options.
- Built-in library version and relevant descriptors.
- Versions/identities of child definitions, or dependency invalidation sufficient to obtain the same effect.

Changing a local stub to a completed chip, adding a local file that shadows a built-in, editing an unsaved editor overlay, or changing the project directory must invalidate affected results. Cache missing-file lookup results only with appropriate directory/source-provider invalidation. Stable instance IDs can use parent instance ID plus part source ordinal; reordered parts may change diagnostic IDs but not circuit results.

## 5. Built-in implementation contract

### 5.1 Supplied library and selection

**[Book: A2.3, pp. 379–381; A2.6, pp. 387–388]** Nand and DFF are supplied primitives. Built-in implementations also provide efficiency, behavioral testing, visualization, and extensions. The book supplies built-in versions of the Hack chips except CPU and Computer. These are identified through HDL interfaces and externally implemented bodies. The actual library folder is `tools/builtInChips` in the second-edition description/reference distribution; do not depend on older tutorial shorthand `builtIn`.

**[Decision]** Provide an explicit built-in manifest with at least name, interface, implementation symbol, factory, combinational dependencies, clock properties, initialization, and script API. Native implementations in Go/Rust/another host language may satisfy the symbolic built-in contract; loading Java `.class` files is not required. A native registry may replace bundled HDL interface files with equivalent descriptors, but must validate the same signatures. Distinguish HDL chip name from `BUILTIN` implementation symbol; their equality is conventional, not a parser requirement.

An unsupported primitive or built-in implementation fails explicitly. Do not silently invent CPU/Computer built-ins, replace a student's chosen HDL with an optimization, or expose every chip's private storage merely because it has the same name as a known built-in. Any acceleration of a user implementation requires separately proven equivalence and an explicit mode.

### 5.2 Descriptor and state isolation

An illustrative descriptor is **[Decision]**:

```text
BuiltinDescriptor
  implementationSymbol
  pins[]: name, direction, width
  clockedInputs[], clockedOutputs[], hasClockActivity
  combinationalDependencies[]: input -> output
  createInstance() -> fresh BuiltinInstance
  scriptState[]: name/form, elementCount, width, readable, writable,
                 readPhase, writePolicy
  methods[]: name, argumentTypes, effects, capabilityRequirements
```

The factory may not share writable arrays or pending state between instances. Validate the HDL-declared interface against the descriptor before construction. Register only explicit dependencies; an empty clocked-pin list still permits clock events. A non-GUI implementation must expose the required state and methods through descriptors rather than constructing windows to gain debug access.

**[Book/Reference]** Some built-ins have GUI side effects. **[Decision]** GUI rendering is optional for the headless backend; memory contents, keyboard events, state APIs, and method effects are not optional when their capabilities are advertised. Give Keyboard a deterministic host-input event source, initially no key; do not poll a physical keyboard during reproducible batch tests unless the caller requests live input.

### 5.3 Script-visible state is a separate API

Follow the sizes, permissions, corrections, and debug-write timing in [tst-spec.md](tst-spec.md), §4. In particular:

- Screen is 8192 16-bit words, not the 16384 suggested by figure A3.2's erroneous range.
- Standalone PC state is 16-bit; CPU `pc` is a 15-bit address view.
- The plain reference Register class and its GUI variants do not expose identical scripting capabilities, despite the textbook's general table.
- `Name[]`/`Name[i]` accesses registered built-in state, not HDL bit slices or a generic hierarchy-navigation language.

**[Decision]** Ambiguous built-in names fail in the default profile. Legacy first-match lookup is available only when explicitly selected. Track stable instance identity internally even if the legacy script syntax cannot name every repeated instance. Do not use script-visible type-name lookup as the runtime's instance storage key.

## 6. Circuit evaluation, time, and state

### 6.1 Observable model

**[Book: A2.1/A2.4, pp. 375, 381–384; A3.2, p. 397]** `PARTS` is an unordered circuit description. Combinational outputs depend on current inputs; clocked dependencies publish updates at the next time unit. The hardware simulator has two phases with time displays `0, 0+, 1, 1+, 2, ...`. Tick moves from `t` to `t+`; tock moves to `t+1`. Wall-clock elapsed time and GUI animation speed do not determine hardware time.

**[Decision]** Keep a nonnegative cycle counter plus a phase enum (`BeforeTick`, `AfterTick`), rather than floating-point time. The runner and GUI read the same clock owned by the simulator. `eval`, `set`, `output`, separators, while tests, and loop boundaries do not increment it. Reject invalid phase order as described in [tst-spec.md](tst-spec.md).

### 6.2 Evaluation algorithm

The following is the selected **[Decision]** algorithm, consistent with the book's circuit model:

1. **Load:** Build fresh instances, initialize baseline state and input pins to zero, initialize host Keyboard to no key and ROM to zero, set time/phase to `0/BeforeTick`, then settle combinational logic. Zero initial sequential state is repository policy; do not describe it as a universal hardware power-on law.
2. **Input `set`:** Validate and write the top-level input, marking affected logic dirty. Do not advance time or capture state. Published calculated pins may remain stale until `eval` or a clock action. A registered debug setter may explicitly re-evaluate as part of its API.
3. **`eval`:** Settle all current combinational dependencies from inputs and published state. Use a topological schedule or an equivalent deterministic propagation algorithm; do not visit parts once in arbitrary source order. No sequential capture occurs.
4. **`tick`:** Settle the pre-capture circuit. Every sequential primitive samples from the same logically pre-capture state into a pending state. Apply all captures only after sampling is complete; a downstream DFF must not sample another DFF's newly captured value. Advance phase to `AfterTick` without incrementing the cycle. Publish only the observable effects its descriptor permits, and settle relevant combinational paths.
5. **`tock`:** Publish all pending clocked outputs/state consistently, settle combinational logic, increment the cycle, and return to `BeforeTick`. Do not resample ordinary clocked inputs at tock. Input edits after tick affect unclocked paths immediately on evaluation and the next capture, not the already captured value.

For ordinary DFFs, `set in 1; tick;` leaves `out` at its old value; `tock;` publishes 1. For two serial DFFs, the second receives the first's *previous* published output in the same cycle. Implementing tick as “sample and expose each primitive in order” violates this contract.

### 6.3 Mixed paths and built-in inspection

**[Book: A2.4, p. 383]** RAM address is not clocked. A change of address followed by `eval` can therefore change its selected output within the same cycle. The presence of a clocked write path must not delay that read path indiscriminately.

**[Decision]** Keep committed state, captured pending state, and published pin values conceptually separate even if an optimized backend packs them differently. For script-visible state inspections, the `readPhase` descriptor declares which state is read. To model the reference built-ins, pending captured register/memory storage may be visible through built-in inspection after tick even while the clocked circuit output has not yet published that update. Do not infer a private-state observation's timing from the similarly named output pin.

For memory-like built-ins, distinguish the storage used for circuit reads from the state exposed to inspection. **[Decision]** In `book-hardware`, RAM/Screen circuit reads use committed storage throughout the cycle, including an address change and `eval` after tick; tock commits the captured write. Inspection handles may expose the captured pending cell after tick, as their descriptor specifies. This closes an edge case that the book does not fully formalize without making ordinary pin evaluation depend on pending state.

**[Reference caveat]** The inspected RAM/Screen implementations update their backing array during `clockUp`, keep the current output until recomputation, and read that same array from `reCompute`. Thus an address change followed by evaluation after tick can expose captured storage before tock. **[Decision]** `legacy-java` models that backing-array behavior and dirty-read scheduling: the capture itself does not request a read recomputation; an unclocked address change marks it dirty, `eval` recomputes a dirty read, and tock recomputes it unconditionally. This is a named compatibility difference from the default committed-storage policy. Verify it with differential phase traces before claiming binary equivalence.

**[Decision]** Normal clock capture and script debug edits have distinct operations. Apply the descriptor-specific debug-write policies from [tst-spec.md](tst-spec.md): generic `Register[]` has the book-profile pending edit; ARegister/DRegister/PC and direct RAM/Screen edits use immediate reference-style updates. A later capture follows normal inputs and may supersede an earlier pending edit. The legacy adapter additionally controls which accessors exist. A device method such as ROM loading edits its device state without advancing the global clock.

### 6.4 Clocked paths and cycles

**[Book: A2.4, pp. 382–384]** A built-in's `CLOCKED` input means changes to that input affect outputs only at the next time unit. A clocked output similarly delays effects of input changes on that output. Nonclocked input-to-output paths can remain combinational. Composite clock behavior follows descendants. Feedback is allowed only when a path through the loop crosses an appropriate clocked pin; instantaneous feedback is an error.

**[Decision]** Construct a dependency graph at sufficient pin/bit/path granularity. Remove delayed edges from the instantaneous graph, propagate dependency summaries through composite definitions, and reject any remaining cyclic strongly connected component, including a self-loop. Do not break all dependencies of a composite simply because one descendant is sequential. A DFF elsewhere in a chip does not legalize an unrelated combinational loop.

Do not use repeated evaluation until values “happen to stabilize” as a substitute for cycle validation. Even a fixed-point-looking combinational feedback network is outside the documented HDL contract. Keep structural definition recursion checks (§4.3) separate from netlist signal-cycle checks.

## 7. Script runner and simulator interface

The conceptual boundary below is **[Decision]**, not a mandated programming API:

```text
Backend
  loadChip(name, SessionContext) -> loadedIdentity or diagnostics
  resolveVariable(reference) -> typed VariableHandle or diagnostics
  read(handle) -> IntegerValue | TextValue
  write(handle, value) -> result
  eval() / tick() / tock() -> result
  invokeBuiltin(target, method, typedArguments) -> result
  clock() -> cycle, phase

ScriptRunner
  parse(source) -> ScriptPlan, diagnostics
  start(plan, backend, io, profile, limits) -> Run
  step(run) -> status/events
  resume(run, mode) -> status/events
  cancel(run) -> status
```

The runner owns control flow, command spans, separators, loops, status, schemas, files, and comparison positions. The backend owns circuit state and hardware time. Compile/load services report structured errors without formatting user interface messages or reading `.cmp` themselves. Formatter/comparator components do not mutate the circuit.

Variable handles record the loaded-chip generation. After `load`, invalidate old handles and rebind stored schema references; never use a stale pointer into the previous circuit. The load/reset policy in [tst-spec.md](tst-spec.md), §5.1, is authoritative: circuit/time and breakpoints reset; streams and their positions remain, and schema source references are retained for rebinding. A fresh entire run resets everything.

The runtime must be usable without `.tst`, for direct programmatic or GUI evaluation. The script runner must be testable with a small fake backend, without constructing real HDL circuits. Editor analysis must be able to ask for interfaces and diagnostics without opening output streams, invoking methods, or starting clock loops.

**[Decision]** CPU/VM backends, if implemented later, reuse the common script controller and I/O but publish different command/variable capabilities. A CPU `ticktock` executes one instruction cycle; a VM `vmstep` executes one VM command; neither is silently accepted by the required hardware backend. See [tst-spec.md](tst-spec.md), §10.

## 8. `.out`, `.cmp`, failures, and test outcomes

**[Book: A3.2, pp. 398–400]** An output-list declares observations and writes a header; `output` writes observations using that schema. Compare mismatches halt script execution. **[Reference]** Headers and data rows pass through the same comparator, which compares exact characters with expected `*` matching one character.

**[Decision]** [tst-spec.md](tst-spec.md), §§7–8, is the single formatting/comparison contract. In particular:

- Preserve field widths, header centering, dividers, letter case, padding, and clipping. Do not serialize an arbitrary CSV/table and expect equivalence.
- Compare generated text to expected text; never replace this with comparing parsed numeric values. Values rendered identically after clipping really do compare identically as text.
- Do not reuse the whitespace-insensitive standalone TextComparer behavior for simulator comparisons.
- Write the failing row before reporting its mismatch. Preserve the partial `.out`, expected/actual line, one-based physical line numbers, and script command span.
- `book-hardware` requires complete expected-file consumption; `legacy-java` allows the inspected controller's expected-prefix behavior. An early expected EOF is handled robustly as failure in both.
- Without `compare-to`, return completion without a correctness verdict. A pause, cancellation, command limit, or compiler error cannot become a pass.

The result structure must distinguish `Passed`, `CompletedUnchecked`, `FailedComparison`, `FailedError`, `Suspended`, `Cancelled`, and `LimitExceeded`. Include profile, source identities/versions, built-in manifest version, command/output counts, comparison coverage, and diagnostics. Never report “all tests pass” merely because no `.cmp` was opened or no output lines were emitted.

**[Decision]** Use structured events for command completion, output row, comparison, status text, clock phase, pause, and diagnostic. The CLI and GUI may render those events differently but must use the same semantics. Exact legacy GUI strings and historical line-number discrepancies are not part of the contract. On terminal completion close streams; on a resumable pause retain their cursors. No failure should silently fall back to another chip implementation or silently continue past the failing command.

## 9. Suggested project boundaries and dependency direction

This directory outline is **[Decision, nonbinding naming]**:

```text
source/       source identities, spans, providers, editor overlays
diagnostic/   codes, severities, related locations
core/         interfaces, values, immutable circuit definitions, runtime simulation
builtins/     native chip factories and descriptors; depends on core
hdl/          lexer, AST, parser, compiler/elaborator; depends on source/core
resolution/   source and definition loading, or included within hdl compilation
tst/          lexer, parser, plan, runner; depends on a backend interface
testio/       fixed-column formatter and comparator
app/          sessions, dependency injection, CLI/GUI adapters
lsp/          editor analysis and navigation
```

Dependencies point toward shared types and interfaces. `core` must not import `hdl`, `tst`, CLI, or LSP. `builtins` must not parse test scripts. `hdl` must not run scripts. `tst` must not inspect arbitrary HDL AST internals to read pins. The application wires a hardware backend to the compiler, built-in registry, core, and runner. If the repository already has a different structure, preserve it while maintaining these responsibilities.

**[Decision]** A recursive definition loader belongs with compilation, or is injected through an interface; it is not a reason to put source-file discovery and parsing inside the simulator core. Conversely, a pure parser should not try to infer child pin directions without consulting semantic interfaces. Reuse one semantic analysis service for CLI validation and LSP diagnostics so their wiring rules cannot diverge.

## 10. Implementing agent contract

Everything in this section is **[Decision]**, not a claim about how the book's original Java code is organized.

### 10.1 Before changing code

The agent MUST read all three sibling specs and the repository's applicable instructions. It must inspect the existing implementation, test fixtures, build tooling, and supported targets before creating replacement modules. Preserve repository APIs and conventions where practical. Record any conflict between these specs and the actual code as a concrete compatibility decision, with a test or source citation.

The agent MUST preserve source/interface names and supplied `.tst`/`.cmp` fixtures. Fix the toolchain or chip implementation rather than editing expected results to obtain a pass. New expected results may be added only for new, independently justified test cases. Do not expand this task into implementing Jack, VM, a full assembler, or a GUI unless those capabilities are actually required and authorized.

### 10.2 Required implementation order

1. Establish typed values, interface descriptors, source spans, and diagnostics.
2. Parse HDL and TST independently, retaining source locations and TST terminators.
3. Implement source-first resolution, built-in descriptors/factories, recursive compilation, and definition-cycle checks.
4. Elaborate bindings, validate widths/drivers, and build immutable circuit plans with path-sensitive combinational dependencies.
5. Implement deterministic runtime construction, combinational evaluation, clock phases, and simultaneous state capture/publication.
6. Expose input/pin/state/method operations through the hardware backend, with explicit permissions and debug-write policies.
7. Implement TST setup, control flow, stepping, observations, exact formatter/comparator, and structured outcomes.
8. Add the required end-to-end fixtures and integrate the shared analysis services into existing CLI/editor surfaces.

This order is a dependency guide, not a requirement for separate pull requests. Do not ship parser-only “support” as full script compatibility, or a simulator that ignores unresolved subchips as a finished elaborator.

### 10.3 Required verification

Use focused unit checks where semantics are isolated, plus actual end-to-end runs. The minimum evidence is:

| Area | Required checks |
|---|---|
| Resolution | Project HDL selected over built-in; missing file falls back; malformed/stub file does not; different project folders do not share the wrong cached definition |
| Recursive loading | Composite child is compiled on demand; shared definitions have independent instances; structural recursion fails with the complete chain |
| HDL | Part-order independence; widths/slices/constants; disjoint bus bindings; aliased output fan-out; no internal slicing; no top-level-output reuse; optional IN/OUT sections |
| Defaults/errors | Unconnected declared pins zero; consumed source-less internal net diagnosed; conflicting drivers and incomplete stubs fail |
| Timing | DFF half-cycle trace; two serial DFFs do not cascade in one tick; RAM fixed-time address changes; post-tick memory inspection; per-chip debug writes; invalid clock order |
| Cycles | Combinational self-loop/multi-chip loop fail; clock-broken feedback succeeds; a DFF on an unrelated path does not hide an instantaneous loop |
| Script/I/O | Run the conformance cases in tst-spec.md §11, including pauses and loops, exact headers, wildcard comparison, EOF, and status distinctions |
| Isolation | Re-run a test fresh with identical outputs; edit a child and invalidate parents; independent chip instances do not share registers or RAM |
| System | Load a circuit through a real .tst, compile descendants, drive inputs, observe output, write .out, compare .cmp, and report a structured result |

Run the supplied relevant Nand2Tetris project tests where legally available. Use projects 1–3 for logic/arithmetic/state and project 5 for CPU/Computer integration when that scope is implemented. Tests that require built-in state access may intentionally differ when a local composite implementation shadows that built-in; use an explicit fixture layout, not hidden substitution. Keep `.out` generated artifacts separate from golden `.cmp` files in test fixtures.

**[Decision]** For differential verification, pin the exact official simulator distribution/source revision, input corpus, profile, output encoding, and comparison mode. Compare success cases as well as diagnostics and phase traces. Record whether each compatibility claim came from book reading, source inspection, or actual reference execution. This document provides the first two kinds of evidence, not the third.

### 10.4 Definition of done

The agent's delivery MUST include:

- The implemented scope/profile and supported built-in manifest.
- Required fixes and meaningful automated checks, with commands/results reported in repository-appropriate form.
- At least one complete passing HDL→TST→OUT→CMP flow and one controlled mismatch/error flow.
- Evidence of state isolation, recursive source resolution, correct simultaneous clock behavior, and exact text comparison.
- A concise list of deliberate deviations and unsupported capabilities. Unimplemented behavior must fail explicitly.
- No hidden rewriting of fixtures, automatic source-to-built-in substitution on errors, shared runtime-state caches, or claims of executing a reference binary that was only read.

A fresh-context agent should be able to resume from these files, inspect the repository, and implement the contract without recovering the earlier conversation. The architecture here is the proposed contract; earlier chat previews do not establish undocumented implementation facts or override repository instructions.

## 11. Source map, provenance, and unresolved limits

Primary evidence from the supplied second-edition PDF:

| Subject | Location |
|---|---|
| Declarative HDL, recursive parts, naming, source structure | A2.1, PDF pp. 373–377 |
| Buses and constants | A2.2, pp. 377–379 |
| Built-in purpose and Hack library exceptions | A2.3, pp. 379–381 |
| Two phases, clocked paths, RAM addressing, feedback | A2.4, pp. 381–384 |
| GUI devices | A2.5, pp. 384–387 |
| Local source versus built-ins, stubs, same-folder workflow | A2.6, pp. 387–389 |
| Unconnected pins, output inspection, internal slicing/fan-out | A2.6, pp. 389–391 |
| Test files, current folder, comments/case, controls | A3.1, pp. 393–395 |
| Hardware commands, output, built-in state, default script | A3.2, pp. 395–403 |
| CPU/VM backend boundaries | A3.3–A3.4, pp. 403–409 |
| Standalone PC description and Screen memory size | Chapter 3 §3.3.4, pp. 88–89; chapter 5 §5.2.4, pp. 134–135, figure 5.4 |
| Chip signatures, including Keyboard | Appendix 4, pp. 410–411 |

These locators identify the supplied PDF's pagination. Use chapter/section and figure identifiers to find the corresponding text in a differently paginated copy.

Supplementary official sources at the pinned commit:

- [GatesManager.java](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/SimulatorsPackageSource/Hack/Gates/GatesManager.java): source-first project/built-in lookup.
- [CompositeGateClass.java](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/SimulatorsPackageSource/Hack/Gates/CompositeGateClass.java): wiring validation, internal sources, output reuse restrictions, dependency graphs.
- [HardwareSimulator.java](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/SimulatorsPackageSource/Hack/HardwareSimulator/HardwareSimulator.java): runtime actions, phase checks, permissions, time, GUI-capable built-in lookup.
- [HackController.java](https://github.com/nand2tetris/nand2tetris_simulator/blob/327b7e4337b31efb815e1c1e3913c0efd299bcf2/HackPackageSource/Hack/Controller/HackController.java): script stepping, output/compare behavior, breakpoints.
- [Built-in source](https://github.com/nand2tetris/nand2tetris_simulator/tree/327b7e4337b31efb815e1c1e3913c0efd299bcf2/BuiltInChipsSource) and [HDL interfaces](https://github.com/nand2tetris/nand2tetris_simulator/tree/327b7e4337b31efb815e1c1e3913c0efd299bcf2/InstallDir/builtInChips): actual PC/Screen widths and debug-write behavior.
- [Official software page](https://www.nand2tetris.org/software): distinguishes tools and the whitespace-insensitive standalone TextComparer. It is supporting context, not a replacement for Appendix 3.

Provenance checksums (SHA-256):

```text
Supplied 427-page second-edition PDF:
f422245b98a126ac3b64a0805a9961432f5628a9da90f074d97b12c303b6e3c6

Existing nand2tetris-hdl-spec.md, unchanged:
d754ba7fda7bd42664a3c1bd245116ce8fe4ec1b7935cc040db67694a796ac9d
```

Known limits: Appendix 3 does not formalize all lexical edge cases, every reset effect, numeric overflow, format clipping, EOF behavior, hierarchy ambiguity, debug-write timing, or error recovery. The decisions in these sibling documents close those gaps for this implementation. More exact emulation of a particular legacy or modern online simulator requires measured differential tests and a separately named compatibility target.

This is an independently organized engineering specification, not an official or publisher-endorsed standard. Source facts, source disagreements, and chosen engineering policies are intentionally kept distinct.
