//! Static analysis of a parsed script against the chips it loads.
//!
//! The analysis follows the script in source order (which is also execution
//! order, as scripts have no backward jumps besides non-nested loops), so a
//! problem reported here is one the script is certain to hit when it reaches
//! that command. It validates file names, variable names and permissions,
//! value widths, condition types, output schemas and built-in method calls.
//! Runtime-only conditions (clock phase order, I/O, comparison) are left to
//! the runner.

use std::sync::Arc;

use crate::builtins::{BuiltinChip, BuiltinLibrary, MethodParameter, StateShape, WritePolicy};
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hdl::definition::{ChipDefinition, NetKind};
use crate::hdl::elaborator::{Elaborator, resolved_from_unit};
use crate::hdl::interface::ChipInterface;
use crate::hdl::resolve::{SourceProvider, is_valid_chip_name};
use crate::hdl::semantic::ResolutionProblem;
use crate::source::Span;
use crate::tst::ast::*;
use crate::tst::parser::check_basename;

/// The result of loading a chip for a script.
#[derive(Debug, Clone)]
pub struct LoadedChip {
    pub interface: Option<Arc<ChipInterface>>,
    pub definition: Option<Arc<ChipDefinition>>,
    pub problem: Option<ResolutionProblem>,
}

/// Loads chips named by `load Xxx.hdl`.
pub trait ChipLoader {
    fn load_chip(&mut self, chip_name: &str) -> LoadedChip;
    fn builtin_library(&self) -> &BuiltinLibrary;
}

impl<P: SourceProvider> ChipLoader for Elaborator<P> {
    fn load_chip(&mut self, chip_name: &str) -> LoadedChip {
        let unit = self.compile(chip_name);
        LoadedChip {
            interface: unit.interface.clone(),
            definition: unit.definition.clone(),
            problem: resolved_from_unit(&unit).problem,
        }
    }

    fn builtin_library(&self) -> &BuiltinLibrary {
        self.builtins()
    }
}

/// How a script variable is accessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableTarget {
    /// The reserved read-only `time` variable (a string such as `3+`).
    Time,
    Pin {
        chip: String,
        /// The HDL spelling of the pin (which may differ in case).
        name: String,
        role: PinRole,
        width: u32,
    },
    BuiltinState {
        chip: String,
        width: u32,
        write: WritePolicy,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinRole {
    Input,
    Output,
    Internal,
}

impl VariableTarget {
    pub fn is_writable(&self) -> bool {
        match self {
            VariableTarget::Time => false,
            VariableTarget::Pin { role, .. } => *role == PinRole::Input,
            VariableTarget::BuiltinState { write, .. } => *write != WritePolicy::ReadOnly,
        }
    }

    fn value_type(&self) -> ValueType {
        match self {
            VariableTarget::Time => ValueType::Text,
            _ => ValueType::Integer,
        }
    }

    fn width(&self) -> Option<u32> {
        match self {
            VariableTarget::Time => None,
            VariableTarget::Pin { width, .. } | VariableTarget::BuiltinState { width, .. } => {
                Some(*width)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Integer,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptTarget {
    /// A chip loaded with `load` or a built-in chip addressed by a method call.
    Chip(String),
    Variable(VariableTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptReference {
    pub span: Span,
    pub target: ScriptTarget,
}

#[derive(Debug, Clone)]
pub struct LoadInfo {
    pub span: Span,
    pub chip_name: String,
    pub interface: Option<Arc<ChipInterface>>,
    pub definition: Option<Arc<ChipDefinition>>,
}

#[derive(Debug, Clone, Default)]
pub struct ScriptAnalysis {
    pub diagnostics: Vec<Diagnostic>,
    pub references: Vec<ScriptReference>,
    pub loads: Vec<LoadInfo>,
}

impl ScriptAnalysis {
    pub fn reference_at(&self, offset: usize) -> Option<&ScriptReference> {
        self.references
            .iter()
            .filter(|r| r.span.contains(offset))
            .min_by_key(|r| r.span.len())
    }

    /// The chip loaded at the point of `offset` in the script.
    pub fn loaded_chip_at(&self, offset: usize) -> Option<&LoadInfo> {
        self.loads.iter().rev().find(|l| l.span.start <= offset)
    }
}

pub fn analyze(script: &Script, loader: &mut dyn ChipLoader) -> ScriptAnalysis {
    let mut analyzer = Analyzer {
        loader,
        current: None,
        reported_no_chip: false,
        has_output_file: false,
        has_output_list: false,
        result: ScriptAnalysis::default(),
    };
    for statement in &script.statements {
        match statement {
            Statement::Simple(s) => analyzer.simple(s),
            Statement::Repeat(r) => {
                for s in &r.body {
                    analyzer.simple(s);
                }
            }
            Statement::While(w) => {
                analyzer.condition(&w.condition);
                for s in &w.body {
                    analyzer.simple(s);
                }
            }
        }
    }
    analyzer.result
}

struct Current {
    name: String,
    interface: Option<Arc<ChipInterface>>,
    definition: Option<Arc<ChipDefinition>>,
}

/// `Ok(None)` means "unknown, but an error has already been reported or
/// cannot be determined" — callers skip further checks silently.
type Resolution = Result<Option<VariableTarget>, Diagnostic>;

struct Analyzer<'l> {
    loader: &'l mut dyn ChipLoader,
    current: Option<Current>,
    reported_no_chip: bool,
    has_output_file: bool,
    has_output_list: bool,
    result: ScriptAnalysis,
}

impl Analyzer<'_> {
    fn error(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.result
            .diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    /// Reports (once) that a command needs a loaded chip. Returns whether a
    /// chip is loaded.
    fn require_chip(&mut self, span: Span) -> bool {
        if self.current.is_some() {
            return true;
        }
        if !self.reported_no_chip {
            self.reported_no_chip = true;
            self.error(
                DiagnosticCode::NoChipLoaded,
                "no chip has been loaded; add `load Xxx.hdl` before this command",
                span,
            );
        }
        false
    }

    fn simple(&mut self, statement: &SimpleStatement) {
        let keyword = statement.keyword.span;
        match &statement.command {
            Command::Load(file) => self.load(file),
            Command::OutputFile(_) => self.has_output_file = true,
            Command::CompareTo(_) => {}
            Command::OutputList(items) => {
                if !self.has_output_file {
                    self.error(
                        DiagnosticCode::NoOutputFile,
                        "`output-list` requires an output file; add `output-file Xxx.out` first",
                        keyword,
                    );
                }
                if self.require_chip(keyword) {
                    for item in items {
                        self.output_item(item);
                    }
                }
                self.has_output_list = true;
            }
            Command::Output => {
                if !self.has_output_list {
                    self.error(
                        DiagnosticCode::NoOutputList,
                        "`output` requires an output schema; add `output-list ...` first",
                        keyword,
                    );
                }
            }
            Command::Set { variable, value } => {
                if !self.require_chip(keyword) {
                    return;
                }
                if let Some(target) = self.variable(variable) {
                    if !target.is_writable() {
                        let what = match &target {
                            VariableTarget::Time => "`time` is read-only".to_string(),
                            VariableTarget::Pin {
                                role: PinRole::Output,
                                ..
                            } => format!(
                                "`{variable}` is an output pin and cannot be set; only input pins are writable"
                            ),
                            VariableTarget::Pin { .. } => format!(
                                "`{variable}` is an internal pin and cannot be set; only input pins are writable"
                            ),
                            VariableTarget::BuiltinState { chip, .. } => {
                                format!("the state of `{chip}` is read-only from scripts")
                            }
                        };
                        self.error(DiagnosticCode::ReadOnlyVariable, what, variable.span);
                    } else if let Some(width) = target.width()
                        && let Err(message) = value.encode(width)
                    {
                        self.error(DiagnosticCode::ValueOutOfRange, message, value.span);
                    }
                }
            }
            Command::Eval | Command::Tick | Command::Tock => {
                self.require_chip(keyword);
            }
            Command::Breakpoint { variable, value } => {
                if !self.require_chip(keyword) {
                    return;
                }
                if let Some(target) = self.variable(variable) {
                    let value_type = self.scalar_type(value);
                    if let Some(value_type) = value_type
                        && value_type != target.value_type()
                    {
                        self.type_mismatch(variable.span.merge(value.span()));
                    }
                }
            }
            Command::BuiltinMethod {
                target,
                method,
                arguments,
            } => {
                if self.require_chip(target.span) {
                    self.builtin_method(target, method, arguments);
                }
            }
            Command::Echo(_)
            | Command::ClearEcho
            | Command::ClearBreakpoints
            | Command::Unsupported { .. } => {}
        }
    }

    fn load(&mut self, file: &FileName) {
        // File name problems were reported by the parser.
        if check_basename(&file.name).is_some() || file.extension() != Some("hdl") {
            self.current = Some(Current {
                name: file.stem().to_string(),
                interface: None,
                definition: None,
            });
            return;
        }
        let name = file.stem().to_string();
        if !is_valid_chip_name(&name) {
            self.error(
                DiagnosticCode::InvalidFileName,
                format!("`{name}` is not a valid chip name"),
                file.span,
            );
            self.current = Some(Current {
                name,
                interface: None,
                definition: None,
            });
            return;
        }

        let loaded = self.loader.load_chip(&name);
        if let Some(problem) = &loaded.problem {
            let d = match problem {
                ResolutionProblem::NotFound { searched } => {
                    let mut d = Diagnostic::error(
                        DiagnosticCode::ChipLoadFailed,
                        format!(
                            "cannot load `{}`: no such file in the script's folder and no built-in chip `{name}`",
                            file.name
                        ),
                        file.span,
                    );
                    for location in searched {
                        d = d.with_related(format!("searched {location}"), None);
                    }
                    d
                }
                ResolutionProblem::HasErrors { related } => {
                    let mut d = Diagnostic::error(
                        DiagnosticCode::ChipLoadFailed,
                        format!("cannot load `{}`: the chip has errors", file.name),
                        file.span,
                    );
                    d.related.extend(related.iter().cloned());
                    d
                }
                ResolutionProblem::Cycle { chain } => Diagnostic::error(
                    DiagnosticCode::ChipLoadFailed,
                    format!(
                        "cannot load `{}`: recursive chip inclusion {}",
                        file.name,
                        chain.join(" -> ")
                    ),
                    file.span,
                ),
            };
            self.result.diagnostics.push(d);
        }
        self.result.references.push(ScriptReference {
            span: file.span,
            target: ScriptTarget::Chip(name.clone()),
        });
        self.result.loads.push(LoadInfo {
            span: file.span,
            chip_name: name.clone(),
            interface: loaded.interface.clone(),
            definition: loaded.definition.clone(),
        });
        self.current = Some(Current {
            name,
            interface: loaded.interface,
            definition: loaded.definition,
        });
    }

    fn output_item(&mut self, item: &OutputItem) {
        let Some(target) = self.variable(&item.variable) else {
            return;
        };
        if target == VariableTarget::Time
            && let Some(format) = item.format
            && format.kind != FormatKind::String
        {
            self.error(
                DiagnosticCode::TypeMismatch,
                "`time` is text and can only be output with `%S`, e.g. `time%S1.4.1`",
                format.span,
            );
        } else if target == VariableTarget::Time && item.format.is_none() {
            self.error(
                DiagnosticCode::TypeMismatch,
                "`time` is text and needs an explicit `%S` format, e.g. `time%S1.4.1`",
                item.variable.span,
            );
        }
    }

    fn type_mismatch(&mut self, span: Span) {
        self.error(
            DiagnosticCode::TypeMismatch,
            "type mismatch: cannot compare text with an integer (`time` is the only text variable; text constants are written %S\"...\")",
            span,
        );
    }

    fn scalar_type(&mut self, scalar: &Scalar) -> Option<ValueType> {
        match scalar {
            Scalar::String(_) => Some(ValueType::Text),
            Scalar::Integer(i) => match i.constant_value() {
                Ok(_) => Some(ValueType::Integer),
                Err(message) => {
                    self.error(DiagnosticCode::ValueOutOfRange, message, i.span);
                    None
                }
            },
        }
    }

    fn operand_type(&mut self, operand: &Operand) -> Option<ValueType> {
        match operand {
            Operand::Scalar(s) => self.scalar_type(s),
            Operand::Variable(v) => {
                if !v.is_time() && !self.require_chip(v.span) {
                    return None;
                }
                self.variable(v).map(|t| t.value_type())
            }
        }
    }

    fn condition(&mut self, condition: &Condition) {
        let left = self.operand_type(&condition.left);
        let right = self.operand_type(&condition.right);
        match (left, right) {
            (Some(l), Some(r)) if l != r => self.type_mismatch(condition.span),
            (Some(ValueType::Text), Some(ValueType::Text))
                if !condition.relation.applies_to_strings() =>
            {
                self.error(
                    DiagnosticCode::TypeMismatch,
                    format!(
                        "text can only be compared with `=` or `<>`, not `{}`",
                        condition.relation.as_str()
                    ),
                    condition.relation_span,
                );
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------------
    // Variable resolution
    // ---------------------------------------------------------------------

    /// Resolves a variable, reporting problems. Returns `None` when unknown.
    fn variable(&mut self, variable: &Variable) -> Option<VariableTarget> {
        match self.resolve(variable) {
            Ok(Some(target)) => {
                self.result.references.push(ScriptReference {
                    span: variable.name.span,
                    target: ScriptTarget::Variable(target.clone()),
                });
                Some(target)
            }
            Ok(None) => None,
            Err(d) => {
                self.result.diagnostics.push(d);
                None
            }
        }
    }

    fn resolve(&self, variable: &Variable) -> Resolution {
        if variable.name.name.eq_ignore_ascii_case("time") {
            if let Some(index) = variable.index {
                return Err(Diagnostic::error(
                    DiagnosticCode::InvalidIndex,
                    "`time` cannot be indexed",
                    index.span,
                ));
            }
            return Ok(Some(VariableTarget::Time));
        }
        let Some(current) = &self.current else {
            return Ok(None);
        };
        match variable.index {
            Some(index) => self.resolve_state(current, variable, index),
            None => self.resolve_pin(current, variable),
        }
    }

    fn resolve_pin(&self, current: &Current, variable: &Variable) -> Resolution {
        let Some(interface) = &current.interface else {
            return Ok(None);
        };
        let mut candidates: Vec<(String, PinRole, u32)> = Vec::new();
        candidates.extend(
            interface
                .inputs
                .iter()
                .map(|p| (p.name.clone(), PinRole::Input, p.width)),
        );
        candidates.extend(
            interface
                .outputs
                .iter()
                .map(|p| (p.name.clone(), PinRole::Output, p.width)),
        );
        if let Some(composite) = current.definition.as_ref().and_then(|d| d.as_composite()) {
            candidates.extend(
                composite
                    .nets
                    .iter()
                    .filter(|n| n.kind == NetKind::Internal)
                    .map(|n| (n.name.clone(), PinRole::Internal, n.width)),
            );
        }

        let name = &variable.name.name;
        let chosen = match candidates.iter().find(|(n, ..)| n == name) {
            Some(c) => c.clone(),
            None => {
                let folded: Vec<_> = candidates
                    .iter()
                    .filter(|(n, ..)| n.eq_ignore_ascii_case(name))
                    .collect();
                match folded.as_slice() {
                    [one] => (*one).clone(),
                    [] => {
                        // Without a definition, internal pins are unknown.
                        if current.definition.is_none() {
                            return Ok(None);
                        }
                        let hint = if self
                            .state_chips(current)
                            .iter()
                            .any(|(c, _)| c.name.eq_ignore_ascii_case(name))
                        {
                            format!(
                                "; to access the state of built-in `{name}`, write `{name}[]` or `{name}[index]`"
                            )
                        } else {
                            String::new()
                        };
                        return Err(Diagnostic::error(
                            DiagnosticCode::UnknownVariable,
                            format!("chip `{}` has no pin named `{name}`{hint}", current.name),
                            variable.span,
                        ));
                    }
                    many => {
                        let names = many
                            .iter()
                            .map(|(n, ..)| format!("`{n}`"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(Diagnostic::error(
                            DiagnosticCode::AmbiguousVariable,
                            format!(
                                "`{name}` is ambiguous: it matches {names} (scripts are case-insensitive); use the exact spelling"
                            ),
                            variable.span,
                        ));
                    }
                }
            }
        };
        Ok(Some(VariableTarget::Pin {
            chip: current.name.clone(),
            name: chosen.0,
            role: chosen.1,
            width: chosen.2,
        }))
    }

    /// Built-in descriptors present in the loaded chip's instance tree, with
    /// their instance counts.
    fn state_chips(&self, current: &Current) -> Vec<(Arc<BuiltinChip>, usize)> {
        let library = self.loader.builtin_library();
        let mut found: Vec<(Arc<BuiltinChip>, usize)> = Vec::new();
        if let Some(definition) = &current.definition {
            definition.visit_instances(&mut |def, _| {
                if let Some(builtin) = def.as_builtin()
                    && let Some(chip) = library.by_symbol(&builtin.symbol)
                {
                    match found.iter_mut().find(|(c, _)| Arc::ptr_eq(c, chip)) {
                        Some((_, count)) => *count += 1,
                        None => found.push((chip.clone(), 1)),
                    }
                }
            });
        }
        found
    }

    /// Finds the unique built-in instance named `name` (exact, then
    /// case-folded) in the loaded chip.
    fn find_builtin(
        &self,
        current: &Current,
        name: &Ident,
        what: &str,
    ) -> Result<Option<Arc<BuiltinChip>>, Diagnostic> {
        if current.definition.is_none() {
            return Ok(None);
        }
        let chips = self.state_chips(current);
        let matching: Vec<&(Arc<BuiltinChip>, usize)> =
            match chips.iter().find(|(c, _)| c.name == name.name) {
                Some(exact) => vec![exact],
                None => chips
                    .iter()
                    .filter(|(c, _)| c.name.eq_ignore_ascii_case(&name.name))
                    .collect(),
            };
        match matching.as_slice() {
            [] => Err(Diagnostic::error(
                DiagnosticCode::UnknownBuiltinChip,
                format!(
                    "the loaded chip `{}` contains no built-in `{}` {what}",
                    current.name, name.name
                ),
                name.span,
            )),
            [(chip, 1)] => Ok(Some(chip.clone())),
            [(chip, count)] => Err(Diagnostic::error(
                DiagnosticCode::AmbiguousBuiltinChip,
                format!(
                    "the loaded chip contains {count} instances of built-in `{}`; scripts cannot tell which one is meant",
                    chip.name
                ),
                name.span,
            )),
            _ => Err(Diagnostic::error(
                DiagnosticCode::AmbiguousBuiltinChip,
                format!(
                    "`{}` matches several built-in chips; use the exact spelling",
                    name.name
                ),
                name.span,
            )),
        }
    }

    fn resolve_state(
        &self,
        current: &Current,
        variable: &Variable,
        index: VariableIndex,
    ) -> Resolution {
        let Some(chip) =
            self.find_builtin(current, &variable.name, "with script-accessible state")?
        else {
            return Ok(None);
        };
        let Some(state) = &chip.script_state else {
            return Err(Diagnostic::error(
                DiagnosticCode::UnknownBuiltinChip,
                format!("built-in `{}` has no script-accessible state", chip.name),
                variable.name.span,
            ));
        };
        match (state.shape, index.value) {
            (StateShape::Scalar, None) => {}
            (StateShape::Scalar, Some(_)) => {
                return Err(Diagnostic::error(
                    DiagnosticCode::InvalidIndex,
                    format!(
                        "`{}` holds a single value; write `{}[]`",
                        chip.name, chip.name
                    ),
                    index.span,
                ));
            }
            (StateShape::Indexed { len }, Some(i)) if i < len as u64 => {}
            (StateShape::Indexed { len }, Some(i)) => {
                return Err(Diagnostic::error(
                    DiagnosticCode::InvalidIndex,
                    format!(
                        "index {i} is out of range for `{}` (0..{})",
                        chip.name,
                        len - 1
                    ),
                    index.span,
                ));
            }
            (StateShape::Indexed { len }, None) => {
                return Err(Diagnostic::error(
                    DiagnosticCode::InvalidIndex,
                    format!("`{}` needs an index between 0 and {}", chip.name, len - 1),
                    index.span,
                ));
            }
        }
        Ok(Some(VariableTarget::BuiltinState {
            chip: chip.name.clone(),
            width: state.width,
            write: state.write,
        }))
    }

    fn builtin_method(&mut self, target: &Ident, method: &Ident, arguments: &[Argument]) {
        let Some(current) = &self.current else {
            return;
        };
        let chip = match self.find_builtin(current, target, "") {
            Ok(Some(chip)) => chip,
            Ok(None) => return,
            Err(mut d) => {
                if d.code == DiagnosticCode::UnknownBuiltinChip {
                    d.message = format!(
                        "unknown command `{}`, or no built-in chip `{}` in the loaded chip",
                        target.name, target.name
                    );
                }
                self.result.diagnostics.push(d);
                return;
            }
        };
        self.result.references.push(ScriptReference {
            span: target.span,
            target: ScriptTarget::Chip(chip.name.clone()),
        });
        let Some(descriptor) = chip.method(&method.name) else {
            let available = chip
                .methods
                .iter()
                .map(|m| format!("`{}`", m.name))
                .collect::<Vec<_>>()
                .join(", ");
            self.error(
                DiagnosticCode::UnknownMethod,
                format!(
                    "built-in `{}` has no method `{}`{}",
                    chip.name,
                    method.name,
                    if available.is_empty() {
                        String::new()
                    } else {
                        format!(" (available: {available})")
                    }
                ),
                method.span,
            );
            return;
        };
        if arguments.len() != descriptor.parameters.len() {
            let span = arguments
                .iter()
                .fold(method.span, |span, a| span.merge(a.span));
            self.error(
                DiagnosticCode::InvalidArguments,
                format!(
                    "`{} {}` takes {} argument{}, but {} were given",
                    chip.name,
                    descriptor.name,
                    descriptor.parameters.len(),
                    if descriptor.parameters.len() == 1 {
                        ""
                    } else {
                        "s"
                    },
                    arguments.len()
                ),
                span,
            );
            return;
        }
        for (argument, parameter) in arguments.iter().zip(&descriptor.parameters) {
            match parameter {
                MethodParameter::FileName { extensions } => {
                    let name = match &argument.kind {
                        ArgumentKind::Word(w) | ArgumentKind::String(w) => w,
                        ArgumentKind::Percent { .. } => {
                            self.error(
                                DiagnosticCode::InvalidArguments,
                                "expected a file name",
                                argument.span,
                            );
                            continue;
                        }
                    };
                    if let Some(problem) = check_basename(name) {
                        self.error(DiagnosticCode::InvalidFileName, problem, argument.span);
                    } else if !extensions.iter().any(|e| {
                        name.rsplit_once('.')
                            .is_some_and(|(s, x)| !s.is_empty() && x == e)
                    }) {
                        let list = extensions
                            .iter()
                            .map(|e| format!("`.{e}`"))
                            .collect::<Vec<_>>()
                            .join(" or ");
                        self.error(
                            DiagnosticCode::InvalidFileName,
                            format!("expected a {list} file name"),
                            argument.span,
                        );
                    }
                }
            }
        }
    }
}
