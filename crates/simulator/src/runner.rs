//! Executes `.tst` scripts against the hardware simulator.
//!
//! The runner owns control flow (statements, loops, terminators, pauses),
//! output schemas, `.out`/`.cmp` streams and breakpoints. The [`Circuit`]
//! owns circuit state and simulated time. Implements the `book-hardware`
//! profile of the test-script specification.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use n2t_core::builtins::{BuiltinChip, MethodParameter, WritePolicy};
use n2t_core::diagnostic::{Diagnostic, has_errors};
use n2t_core::hdl::definition::NetKind;
use n2t_core::hdl::resolve::{FsSourceProvider, SourceProvider};
use n2t_core::hdl::{ChipUnit, Elaborator};
use n2t_core::source::{LineIndex, Span};
use n2t_core::tst::ast::*;

use crate::circuit::{Circuit, PinHandle};
use crate::format::{self, Comparator, Mismatch, Value};
use crate::primitives::BuiltinRegistry;

#[derive(Debug, Clone, Default)]
pub struct RunnerOptions {
    /// Stop with [`Outcome::LimitExceeded`] after this many simple commands.
    pub max_commands: Option<u64>,
    /// Set to `true` from another thread to cancel the run.
    pub cancel: Option<Arc<AtomicBool>>,
}

/// An error that stops a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunError {
    pub message: String,
    /// The span of the failing command in the script.
    pub span: Option<Span>,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendReason {
    /// A command terminated by `!`.
    Pause,
    /// A breakpoint's condition became true.
    Breakpoint,
    /// End of a `;` step while single-stepping.
    StepEnd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The script completed and every comparison passed, consuming the whole
    /// compare file.
    Passed {
        compared_lines: usize,
    },
    /// The script completed without a compare file.
    CompletedUnchecked,
    FailedComparison {
        mismatch: Mismatch,
        span: Option<Span>,
    },
    FailedError(RunError),
    /// Resumable: call [`ScriptRunner::run`] or [`ScriptRunner::step`] again.
    Suspended {
        reason: SuspendReason,
        span: Span,
    },
    Cancelled,
    LimitExceeded,
}

impl Outcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Outcome::Passed { .. } | Outcome::CompletedUnchecked)
    }

    pub fn is_terminal(&self) -> bool {
        !matches!(self, Outcome::Suspended { .. })
    }
}

/// Things a front-end may want to show while a script runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Echo(String),
    ClearEcho,
    /// A header or data line written to the output file.
    OutputLine(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Run,
    Step,
}

#[derive(Debug, Clone, Copy, Default)]
struct Cursor {
    statement: usize,
    body: usize,
    iteration: u64,
    condition_checked: bool,
}

struct OutputStream {
    path: PathBuf,
    writer: BufWriter<File>,
}

#[derive(Debug, Clone)]
enum Handle {
    Time,
    Pin(PinHandle),
    State {
        chip: String,
        instance: usize,
        index: usize,
        width: u32,
        write: WritePolicy,
    },
}

struct Breakpoint {
    variable: Variable,
    value: Scalar,
    latched: bool,
}

enum Stop {
    Error(RunError),
    Mismatch(Mismatch),
}

impl From<RunError> for Stop {
    fn from(e: RunError) -> Self {
        Stop::Error(e)
    }
}

fn error(message: impl Into<String>, span: Span) -> RunError {
    RunError {
        message: message.into(),
        span: Some(span),
    }
}

/// Runs one script. A runner is single-use: create a new one for a fresh run.
pub struct ScriptRunner<P: SourceProvider> {
    project_dir: PathBuf,
    script_path: Option<PathBuf>,
    elaborator: Elaborator<P>,
    registry: BuiltinRegistry,
    script: Arc<Script>,
    options: RunnerOptions,

    cursor: Cursor,
    circuit: Option<Circuit>,
    loaded: Option<String>,
    output: Option<OutputStream>,
    compare: Option<(PathBuf, Comparator)>,
    schema: Option<Vec<OutputItem>>,
    breakpoints: Vec<Breakpoint>,
    echo: Option<String>,
    events: Vec<Event>,
    commands: u64,
    current_span: Span,
    finished: Option<Outcome>,
}

/// Why a script could not be started.
#[derive(Debug)]
pub enum StartError {
    Io(PathBuf, std::io::Error),
    /// Lexical/syntax errors. Nothing has been executed or written.
    Syntax(Vec<Diagnostic>),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Io(path, e) => write!(f, "cannot read `{}`: {e}", path.display()),
            StartError::Syntax(d) => write!(
                f,
                "the script has {} syntax error(s)",
                d.iter().filter(|d| d.is_error()).count()
            ),
        }
    }
}

impl std::error::Error for StartError {}

impl ScriptRunner<FsSourceProvider> {
    /// Opens a script file; chips resolve against the script's folder.
    pub fn open(
        script_path: &Path,
        builtins: Arc<n2t_core::builtins::BuiltinLibrary>,
        registry: BuiltinRegistry,
        options: RunnerOptions,
    ) -> Result<Self, StartError> {
        let canonical = script_path
            .canonicalize()
            .map_err(|e| StartError::Io(script_path.to_path_buf(), e))?;
        let text = std::fs::read_to_string(&canonical)
            .map_err(|e| StartError::Io(canonical.clone(), e))?;
        let dir = canonical
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let elaborator = Elaborator::new(FsSourceProvider::new(&dir), builtins);
        let mut runner = Self::new(&text, dir, elaborator, registry, options)?;
        runner.script_path = Some(canonical);
        Ok(runner)
    }
}

impl<P: SourceProvider> ScriptRunner<P> {
    /// Parses `script` (rejecting syntax errors) and prepares a run whose
    /// companion files resolve against `project_dir`.
    pub fn new(
        script: &str,
        project_dir: PathBuf,
        elaborator: Elaborator<P>,
        registry: BuiltinRegistry,
        options: RunnerOptions,
    ) -> Result<Self, StartError> {
        let parsed = n2t_core::tst::parse(script);
        if has_errors(&parsed.diagnostics) {
            return Err(StartError::Syntax(parsed.diagnostics));
        }
        Ok(Self {
            project_dir,
            script_path: None,
            elaborator,
            registry,
            script: Arc::new(parsed.script),
            options,
            cursor: Cursor::default(),
            circuit: None,
            loaded: None,
            output: None,
            compare: None,
            schema: None,
            breakpoints: Vec::new(),
            echo: None,
            events: Vec::new(),
            commands: 0,
            current_span: Span::default(),
            finished: None,
        })
    }

    pub fn circuit(&self) -> Option<&Circuit> {
        self.circuit.as_ref()
    }

    pub fn circuit_mut(&mut self) -> Option<&mut Circuit> {
        self.circuit.as_mut()
    }

    pub fn loaded_chip(&self) -> Option<&str> {
        self.loaded.as_deref()
    }

    pub fn echo(&self) -> Option<&str> {
        self.echo.as_deref()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn commands_executed(&self) -> u64 {
        self.commands
    }

    /// The path of the current output file, if any.
    pub fn output_path(&self) -> Option<&Path> {
        self.output.as_ref().map(|o| o.path.as_path())
    }

    pub fn elaborator(&self) -> &Elaborator<P> {
        &self.elaborator
    }

    /// The span of the most recently executed command.
    pub fn current_span(&self) -> Span {
        self.current_span
    }

    /// Runs until completion, failure or suspension.
    pub fn run(&mut self) -> Outcome {
        self.execute(Mode::Run)
    }

    /// Runs until the end of the current `;` step (or an earlier stop).
    pub fn step(&mut self) -> Outcome {
        self.execute(Mode::Step)
    }

    fn execute(&mut self, mode: Mode) -> Outcome {
        if let Some(outcome) = &self.finished {
            return outcome.clone();
        }
        let outcome = self.execute_inner(mode);
        if outcome.is_terminal() {
            if let Some(output) = &mut self.output {
                let _ = output.writer.flush();
            }
            self.finished = Some(outcome.clone());
        }
        outcome
    }

    fn execute_inner(&mut self, mode: Mode) -> Outcome {
        let script = self.script.clone();
        loop {
            if self
                .options
                .cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
            {
                return Outcome::Cancelled;
            }
            if self
                .options
                .max_commands
                .is_some_and(|max| self.commands >= max)
            {
                return Outcome::LimitExceeded;
            }

            let Some(statement) = script.statements.get(self.cursor.statement) else {
                return self.complete();
            };
            let (simple, body_len) = match statement {
                Statement::Simple(s) => (s, None),
                Statement::Repeat(r) => {
                    let done = r.body.is_empty()
                        || (self.cursor.body == 0
                            && r.count.is_some_and(|c| self.cursor.iteration >= c.value));
                    if done {
                        self.next_statement();
                        continue;
                    }
                    (&r.body[self.cursor.body], Some(r.body.len()))
                }
                Statement::While(w) => {
                    if w.body.is_empty() {
                        self.next_statement();
                        continue;
                    }
                    if self.cursor.body == 0 && !self.cursor.condition_checked {
                        match self.condition(&w.condition) {
                            Ok(true) => self.cursor.condition_checked = true,
                            Ok(false) => {
                                self.next_statement();
                                continue;
                            }
                            Err(e) => return Outcome::FailedError(e),
                        }
                    }
                    (&w.body[self.cursor.body], Some(w.body.len()))
                }
            };

            self.current_span = simple.span;
            match self.command(simple) {
                Ok(()) => {}
                Err(Stop::Error(e)) => return Outcome::FailedError(e),
                Err(Stop::Mismatch(mismatch)) => {
                    return Outcome::FailedComparison {
                        mismatch,
                        span: Some(simple.span),
                    };
                }
            }
            self.commands += 1;

            match body_len {
                None => self.next_statement(),
                Some(len) => {
                    self.cursor.body += 1;
                    if self.cursor.body == len {
                        self.cursor.body = 0;
                        self.cursor.iteration += 1;
                        self.cursor.condition_checked = false;
                    }
                }
            }

            match self.check_breakpoints() {
                Ok(true) => {
                    return Outcome::Suspended {
                        reason: SuspendReason::Breakpoint,
                        span: simple.span,
                    };
                }
                Ok(false) => {}
                Err(e) => return Outcome::FailedError(e),
            }
            match simple.terminator.map(|t| t.kind) {
                Some(TerminatorKind::Bang) => {
                    return Outcome::Suspended {
                        reason: SuspendReason::Pause,
                        span: simple.span,
                    };
                }
                Some(TerminatorKind::Semicolon) if mode == Mode::Step => {
                    return Outcome::Suspended {
                        reason: SuspendReason::StepEnd,
                        span: simple.span,
                    };
                }
                _ => {}
            }
        }
    }

    fn next_statement(&mut self) {
        self.cursor = Cursor {
            statement: self.cursor.statement + 1,
            ..Cursor::default()
        };
    }

    fn complete(&mut self) -> Outcome {
        if let Some(output) = &mut self.output
            && let Err(e) = output.writer.flush()
        {
            return Outcome::FailedError(RunError {
                message: format!("cannot write `{}`: {e}", output.path.display()),
                span: None,
            });
        }
        match &self.compare {
            None => Outcome::CompletedUnchecked,
            Some((path, comparator)) => {
                if comparator.compared() == 0 {
                    return Outcome::FailedError(RunError {
                        message: format!(
                            "a compare file (`{}`) was opened but no output was compared",
                            path.display()
                        ),
                        span: None,
                    });
                }
                match comparator.finish() {
                    Ok(()) => Outcome::Passed {
                        compared_lines: comparator.compared(),
                    },
                    Err(mismatch) => Outcome::FailedComparison {
                        mismatch,
                        span: None,
                    },
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Commands
    // ---------------------------------------------------------------------

    fn circuit_or_error(&mut self, span: Span) -> Result<&mut Circuit, RunError> {
        self.circuit
            .as_mut()
            .ok_or_else(|| error("no chip is loaded; use `load Xxx.hdl` first", span))
    }

    fn command(&mut self, statement: &SimpleStatement) -> Result<(), Stop> {
        let span = statement.command_span;
        match &statement.command {
            Command::Load(file) => self.load(file)?,
            Command::OutputFile(file) => self.output_file(file)?,
            Command::CompareTo(file) => {
                let path = self.project_dir.join(&file.name);
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    error(format!("cannot read `{}`: {e}", path.display()), file.span)
                })?;
                self.compare = Some((path, Comparator::new(&text)));
            }
            Command::OutputList(items) => {
                if self.output.is_none() {
                    return Err(error(
                        "`output-list` requires an output file; use `output-file Xxx.out` first",
                        span,
                    )
                    .into());
                }
                for item in items {
                    self.resolve(&item.variable)?;
                }
                self.schema = Some(items.clone());
                let labels: Vec<(String, OutputFormat)> = items
                    .iter()
                    .map(|i| (item_label(i), i.effective_format()))
                    .collect();
                let line = format::header(labels.iter().map(|(n, f)| (n.as_str(), *f)));
                self.output_line(&line)?;
            }
            Command::Output => {
                let Some(schema) = self.schema.clone() else {
                    return Err(error(
                        "`output` requires an output schema; use `output-list` first",
                        span,
                    )
                    .into());
                };
                let mut values = Vec::with_capacity(schema.len());
                for item in &schema {
                    let handle = self.resolve(&item.variable)?;
                    values.push((self.read(&handle), item.effective_format()));
                }
                let line = format::row(values.iter().map(|(v, f)| (v, *f)));
                self.output_line(&line)?;
            }
            Command::Set { variable, value } => self.set(variable, value)?,
            Command::Eval => self.circuit_or_error(span)?.eval(),
            Command::Tick => {
                let circuit = self.circuit_or_error(span)?;
                circuit.tick().map_err(|e| error(e.to_string(), span))?;
            }
            Command::Tock => {
                let circuit = self.circuit_or_error(span)?;
                circuit.tock().map_err(|e| error(e.to_string(), span))?;
            }
            Command::Echo(text) => {
                self.echo = Some(text.value.clone());
                self.events.push(Event::Echo(text.value.clone()));
            }
            Command::ClearEcho => {
                self.echo = None;
                self.events.push(Event::ClearEcho);
            }
            Command::Breakpoint { variable, value } => {
                let handle = self.resolve(variable)?;
                let current = self.read(&handle);
                // Validate the value's type once at registration.
                breakpoint_matches(&current, value).map_err(|m| error(m, span))?;
                let duplicate = self.breakpoints.iter().any(|b| {
                    b.variable
                        .to_string()
                        .eq_ignore_ascii_case(&variable.to_string())
                        && scalar_text(&b.value) == scalar_text(value)
                });
                if !duplicate {
                    self.breakpoints.push(Breakpoint {
                        variable: variable.clone(),
                        value: value.clone(),
                        latched: false,
                    });
                }
            }
            Command::ClearBreakpoints => self.breakpoints.clear(),
            Command::BuiltinMethod {
                target,
                method,
                arguments,
            } => self.builtin_method(target, method, arguments, span)?,
            Command::Unsupported { name, .. } => {
                return Err(error(
                    format!("`{}` is not supported by the hardware simulator", name.name),
                    name.span,
                )
                .into());
            }
        }
        Ok(())
    }

    fn load(&mut self, file: &FileName) -> Result<(), RunError> {
        let name = file.stem().to_string();
        let unit = self.elaborator.compile(&name);
        let Some(definition) = unit.definition.clone() else {
            return Err(error(load_failure(&file.name, &unit), file.span));
        };
        let circuit = Circuit::new(definition, &self.registry)
            .map_err(|e| error(e.to_string(), file.span))?;
        self.circuit = Some(circuit);
        self.loaded = Some(name);
        self.breakpoints.clear();
        Ok(())
    }

    fn output_file(&mut self, file: &FileName) -> Result<(), RunError> {
        let path = self.project_dir.join(&file.name);
        let canonical = path.canonicalize().ok();
        let protected = [
            self.script_path.as_deref(),
            self.compare.as_ref().map(|(p, _)| p.as_path()),
        ];
        if canonical.is_some()
            && protected
                .iter()
                .flatten()
                .any(|p| p.canonicalize().ok() == canonical)
        {
            return Err(error(
                format!("refusing to overwrite `{}`", path.display()),
                file.span,
            ));
        }
        if let Some(mut old) = self.output.take() {
            let _ = old.writer.flush();
        }
        let handle = File::create(&path).map_err(|e| {
            error(
                format!("cannot create `{}`: {e}", path.display()),
                file.span,
            )
        })?;
        self.output = Some(OutputStream {
            path,
            writer: BufWriter::new(handle),
        });
        Ok(())
    }

    fn output_line(&mut self, line: &str) -> Result<(), Stop> {
        let span = self.current_span;
        let Some(output) = &mut self.output else {
            return Err(error(
                "no output file is open; use `output-file Xxx.out` first",
                span,
            )
            .into());
        };
        writeln!(output.writer, "{line}").map_err(|e| {
            error(
                format!("cannot write `{}`: {e}", output.path.display()),
                span,
            )
        })?;
        self.events.push(Event::OutputLine(line.to_string()));
        if let Some((_, comparator)) = &mut self.compare
            && let Err(mismatch) = comparator.compare(line)
        {
            let _ = output.writer.flush();
            return Err(Stop::Mismatch(mismatch));
        }
        Ok(())
    }

    fn set(&mut self, variable: &Variable, value: &IntegerLiteral) -> Result<(), RunError> {
        let handle = self.resolve(variable)?;
        let span = variable.span;
        match handle {
            Handle::Time => Err(error("`time` is read-only", span)),
            Handle::Pin(pin) => {
                if !matches!(pin.kind, NetKind::Input(_)) {
                    return Err(error(
                        format!("`{variable}` is not an input pin and cannot be set"),
                        span,
                    ));
                }
                let bits = value.encode(pin.width).map_err(|m| error(m, value.span))?;
                self.circuit_or_error(span)?.write(&pin, bits);
                Ok(())
            }
            Handle::State {
                chip,
                instance,
                index,
                width,
                write,
            } => {
                if write == WritePolicy::ReadOnly {
                    return Err(error(format!("the state of `{chip}` is read-only"), span));
                }
                let bits = value.encode(width).map_err(|m| error(m, value.span))?;
                let circuit = self.circuit_or_error(span)?;
                if !circuit.write_state(instance, index, bits) {
                    return Err(error(format!("cannot write `{variable}`"), span));
                }
                Ok(())
            }
        }
    }

    fn builtin_method(
        &mut self,
        target: &Ident,
        method: &Ident,
        arguments: &[Argument],
        span: Span,
    ) -> Result<(), RunError> {
        let (instance, chip) = self.find_builtin(target, |c| !c.methods.is_empty())?;
        let Some(descriptor) = chip.method(&method.name) else {
            return Err(error(
                format!("built-in `{}` has no method `{}`", chip.name, method.name),
                method.span,
            ));
        };
        if arguments.len() != descriptor.parameters.len() {
            return Err(error(
                format!(
                    "`{} {}` takes {} argument(s)",
                    chip.name,
                    descriptor.name,
                    descriptor.parameters.len()
                ),
                span,
            ));
        }
        // The only standard method is `ROM32K load file`.
        let [MethodParameter::FileName { .. }] = descriptor.parameters.as_slice() else {
            return Err(error(
                format!("method `{}` is not supported", descriptor.name),
                method.span,
            ));
        };
        let argument = &arguments[0];
        let name = match &argument.kind {
            ArgumentKind::Word(w) | ArgumentKind::String(w) => w.clone(),
            ArgumentKind::Percent { .. } => {
                return Err(error("expected a file name", argument.span));
            }
        };
        if let Some(problem) = n2t_core::tst::parser::check_basename(&name) {
            return Err(error(problem, argument.span));
        }
        let path = self.project_dir.join(&name);
        if name.ends_with(".asm") {
            return Err(error(
                "loading `.asm` programs requires an assembler, which is not available; assemble the program and load the `.hack` file",
                argument.span,
            ));
        }
        if !name.ends_with(".hack") {
            return Err(error("expected a `.hack` program file", argument.span));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| {
            error(
                format!("cannot read `{}`: {e}", path.display()),
                argument.span,
            )
        })?;
        let words =
            parse_hack(&text).map_err(|m| error(format!("`{name}`: {m}"), argument.span))?;
        let circuit = self.circuit_or_error(span)?;
        if !circuit.load_program(instance, &words) {
            return Err(error(
                format!("cannot load `{name}` into `{}`", chip.name),
                argument.span,
            ));
        }
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Variables
    // ---------------------------------------------------------------------

    fn find_builtin(
        &self,
        name: &Ident,
        filter: impl Fn(&BuiltinChip) -> bool,
    ) -> Result<(usize, Arc<BuiltinChip>), RunError> {
        let circuit = self
            .circuit
            .as_ref()
            .ok_or_else(|| error("no chip is loaded", name.span))?;
        let library = self.elaborator.builtins();
        let candidates: Vec<(usize, Arc<BuiltinChip>)> = circuit
            .primitives()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let chip = library.by_symbol(&p.symbol)?;
                (filter(chip) && chip.name == p.chip_name).then(|| (i, chip.clone()))
            })
            .collect();
        let exact: Vec<_> = candidates
            .iter()
            .filter(|(_, c)| c.name == name.name)
            .collect();
        let matching = if exact.is_empty() {
            candidates
                .iter()
                .filter(|(_, c)| c.name.eq_ignore_ascii_case(&name.name))
                .collect()
        } else {
            exact
        };
        match matching.as_slice() {
            [] => Err(error(
                format!("the loaded chip contains no built-in `{}`", name.name),
                name.span,
            )),
            [one] => Ok((*one).clone()),
            [first, ..] => Err(error(
                format!(
                    "the loaded chip contains {} instances of built-in `{}`; the reference is ambiguous",
                    matching.len(),
                    first.1.name
                ),
                name.span,
            )),
        }
    }

    fn resolve(&self, variable: &Variable) -> Result<Handle, RunError> {
        let span = variable.span;
        if variable.is_time() {
            return Ok(Handle::Time);
        }
        let circuit = self
            .circuit
            .as_ref()
            .ok_or_else(|| error("no chip is loaded; use `load Xxx.hdl` first", span))?;
        if let Some(index) = variable.index {
            let (instance, chip) =
                self.find_builtin(&variable.name, |c| c.script_state.is_some())?;
            let state = chip.script_state.as_ref().expect("filtered");
            let index = match (state.shape, index.value) {
                (n2t_core::builtins::StateShape::Scalar, None) => 0,
                (n2t_core::builtins::StateShape::Indexed { len }, Some(i)) if i < len as u64 => {
                    i as usize
                }
                _ => {
                    return Err(error(
                        format!("invalid index for `{}`", chip.name),
                        index.span,
                    ));
                }
            };
            return Ok(Handle::State {
                chip: chip.name.clone(),
                instance,
                index,
                width: state.width,
                write: state.write,
            });
        }
        let name = &variable.name.name;
        let pins = circuit.pins();
        if let Some(pin) = pins.iter().find(|p| p.name == *name) {
            return Ok(Handle::Pin(pin.clone()));
        }
        let folded: Vec<&PinHandle> = pins
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect();
        match folded.as_slice() {
            [one] => Ok(Handle::Pin((*one).clone())),
            [] => Err(error(
                format!("the loaded chip has no pin named `{name}`"),
                span,
            )),
            _ => Err(error(
                format!("`{name}` is ambiguous; use the exact spelling"),
                span,
            )),
        }
    }

    fn read(&self, handle: &Handle) -> Value {
        let Some(circuit) = &self.circuit else {
            return Value::Text(String::new());
        };
        match handle {
            Handle::Time => Value::Text(circuit.time().to_string()),
            Handle::Pin(pin) => Value::Int {
                bits: circuit.read(pin),
                width: pin.width,
            },
            Handle::State {
                instance,
                index,
                width,
                ..
            } => Value::Int {
                bits: circuit.read_state(*instance, *index).unwrap_or(0),
                width: *width,
            },
        }
    }

    fn operand(&self, operand: &Operand) -> Result<Value, RunError> {
        match operand {
            Operand::Variable(v) => Ok(self.read(&self.resolve(v)?)),
            Operand::Scalar(Scalar::String(s)) => Ok(Value::Text(s.value.clone())),
            Operand::Scalar(Scalar::Integer(i)) => {
                let value = i.constant_value().map_err(|m| error(m, i.span))?;
                Ok(Value::Int {
                    bits: value as u64,
                    width: 64,
                })
            }
        }
    }

    fn condition(&self, condition: &Condition) -> Result<bool, RunError> {
        let left = self.operand(&condition.left)?;
        let right = self.operand(&condition.right)?;
        let relation = condition.relation;
        match (&left, &right) {
            (Value::Text(a), Value::Text(b)) => {
                if !relation.applies_to_strings() {
                    return Err(error(
                        "text can only be compared with `=` or `<>`",
                        condition.relation_span,
                    ));
                }
                Ok(relation.evaluate(a, b))
            }
            (Value::Int { .. }, Value::Int { .. }) => {
                Ok(relation.evaluate(&int_scalar(&left), &int_scalar(&right)))
            }
            _ => Err(error("cannot compare text with an integer", condition.span)),
        }
    }

    /// Returns whether a breakpoint fired.
    fn check_breakpoints(&mut self) -> Result<bool, RunError> {
        let mut fired = false;
        for i in 0..self.breakpoints.len() {
            let handle = self.resolve(&self.breakpoints[i].variable)?;
            let current = self.read(&handle);
            let matches = breakpoint_matches(&current, &self.breakpoints[i].value)
                .map_err(|m| error(m, self.breakpoints[i].variable.span))?;
            let breakpoint = &mut self.breakpoints[i];
            if matches && !breakpoint.latched {
                breakpoint.latched = true;
                fired = true;
            } else if !matches {
                breakpoint.latched = false;
            }
        }
        Ok(fired)
    }
}

/// The header label of an output item: its script spelling, including any index.
fn item_label(item: &OutputItem) -> String {
    item.variable.to_string()
}

/// Numeric value for comparisons: constants are stored as full 64-bit
/// patterns of their signed value; pins follow [`Value::scalar`].
fn int_scalar(value: &Value) -> i64 {
    match *value {
        Value::Int { bits, width: 64 } => bits as i64,
        ref v => v.scalar().unwrap_or(0),
    }
}

fn scalar_text(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Integer(i) => i
            .constant_value()
            .map(|v| v.to_string())
            .unwrap_or_default(),
        Scalar::String(s) => format!("\"{}\"", s.value),
    }
}

fn breakpoint_matches(current: &Value, value: &Scalar) -> Result<bool, String> {
    match (current, value) {
        (Value::Text(t), Scalar::String(s)) => Ok(*t == s.value),
        (Value::Int { .. }, Scalar::Integer(i)) => {
            Ok(current.scalar() == Some(i.constant_value()?))
        }
        _ => Err("the breakpoint value's type does not match the variable (`time` is text)".into()),
    }
}

/// Parses a `.hack` program: one 16-digit binary word per non-empty line.
pub fn parse_hack(text: &str) -> Result<Vec<u16>, String> {
    let mut words = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() != 16 || !line.bytes().all(|b| b == b'0' || b == b'1') {
            return Err(format!("line {} is not a 16-bit binary word", number + 1));
        }
        words.push(u16::from_str_radix(line, 2).expect("validated"));
    }
    if words.len() > 32768 {
        return Err(format!(
            "the program has {} words; ROM32K holds 32768",
            words.len()
        ));
    }
    Ok(words)
}

/// Summarises why a chip failed to load, with locations.
pub fn load_failure(file_name: &str, unit: &ChipUnit) -> String {
    let mut message = format!("cannot load `{file_name}`");
    let errors: Vec<&Diagnostic> = unit.diagnostics.iter().filter(|d| d.is_error()).collect();
    if errors.is_empty() {
        return message;
    }
    message.push(':');
    let text = unit.text().unwrap_or("");
    let lines = LineIndex::new(text);
    let source = unit.source_id();
    for d in errors.iter().take(10) {
        match (source, d.span) {
            (Some(source), Some(span)) => {
                let (line, column) = lines.line_col_one_based(text, span.start);
                message.push_str(&format!("\n  {source}:{line}:{column}: {}", d.message));
            }
            (Some(source), None) => message.push_str(&format!("\n  {source}: {}", d.message)),
            (None, _) => message.push_str(&format!("\n  {}", d.message)),
        }
        for related in &d.related {
            message.push_str(&format!("\n    note: {}", related.message));
        }
    }
    if errors.len() > 10 {
        message.push_str(&format!("\n  … and {} more", errors.len() - 10));
    }
    message
}
