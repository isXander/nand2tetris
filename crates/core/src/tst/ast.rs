//! Syntax tree for `.tst` scripts. The tree doubles as the execution plan:
//! statements are in source order and simple commands keep their terminators.

use std::fmt;

use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The name exactly as written.
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct Script {
    pub statements: Vec<Statement>,
}

impl Script {
    /// Every simple statement in source order, including loop bodies.
    pub fn simple_statements(&self) -> impl Iterator<Item = &SimpleStatement> {
        self.statements
            .iter()
            .flat_map(|s| -> Box<dyn Iterator<Item = &SimpleStatement>> {
                match s {
                    Statement::Simple(simple) => Box::new(std::iter::once(simple)),
                    Statement::Repeat(r) => Box::new(r.body.iter()),
                    Statement::While(w) => Box::new(w.body.iter()),
                }
            })
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    Simple(SimpleStatement),
    Repeat(RepeatBlock),
    While(WhileBlock),
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Simple(s) => s.span,
            Statement::Repeat(r) => r.span,
            Statement::While(w) => w.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimpleStatement {
    pub command: Command,
    /// Span of the command, excluding the terminator.
    pub command_span: Span,
    /// The command name as written (e.g. `output-list`, `ROM32K`).
    pub keyword: Ident,
    /// `None` only in recovered trees with a syntax error.
    pub terminator: Option<Terminator>,
    /// Span including the terminator.
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminatorKind {
    /// `,` — continue within the current simulation step.
    Comma,
    /// `;` — end the simulation step.
    Semicolon,
    /// `!` — suspend execution until resumed.
    Bang,
}

#[derive(Debug, Clone)]
pub struct RepeatBlock {
    /// `None` repeats forever. Always positive when present.
    pub count: Option<Count>,
    pub body: Vec<SimpleStatement>,
    pub keyword_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub value: u64,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileBlock {
    pub condition: Condition,
    pub body: Vec<SimpleStatement>,
    pub keyword_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Condition {
    pub left: Operand,
    pub relation: Relation,
    pub relation_span: Span,
    pub right: Operand,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Equal => "=",
            Relation::NotEqual => "<>",
            Relation::Less => "<",
            Relation::Greater => ">",
            Relation::LessEqual => "<=",
            Relation::GreaterEqual => ">=",
        }
    }

    /// Whether this relation is valid for strings (only `=` and `<>`).
    pub fn applies_to_strings(self) -> bool {
        matches!(self, Relation::Equal | Relation::NotEqual)
    }

    pub fn evaluate<T: Ord>(self, left: &T, right: &T) -> bool {
        match self {
            Relation::Equal => left == right,
            Relation::NotEqual => left != right,
            Relation::Less => left < right,
            Relation::Greater => left > right,
            Relation::LessEqual => left <= right,
            Relation::GreaterEqual => left >= right,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Operand {
    Variable(Variable),
    Scalar(Scalar),
}

impl Operand {
    pub fn span(&self) -> Span {
        match self {
            Operand::Variable(v) => v.span,
            Operand::Scalar(s) => s.span(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Scalar {
    Integer(IntegerLiteral),
    String(StringLiteral),
}

impl Scalar {
    pub fn span(&self) -> Span {
        match self {
            Scalar::Integer(i) => i.span,
            Scalar::String(s) => s.span,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    /// `load Xxx.hdl`
    Load(FileName),
    /// `output-file Xxx.out`
    OutputFile(FileName),
    /// `compare-to Xxx.cmp`
    CompareTo(FileName),
    /// `output-list a%B1.1.1 b out`
    OutputList(Vec<OutputItem>),
    /// `set variable value`
    Set {
        variable: Variable,
        value: IntegerLiteral,
    },
    Eval,
    Output,
    Tick,
    Tock,
    /// `echo "text"`
    Echo(StringLiteral),
    ClearEcho,
    /// `breakpoint variable value`
    Breakpoint {
        variable: Variable,
        value: Scalar,
    },
    ClearBreakpoints,
    /// `BuiltInChip method arguments...`, e.g. `ROM32K load Max.hack`.
    BuiltinMethod {
        target: Ident,
        method: Ident,
        arguments: Vec<Argument>,
    },
    /// A command from another dialect (`ticktock`, `vmstep`) — parsed so it
    /// can be reported, never executable on the hardware backend.
    Unsupported {
        name: Ident,
        arguments: Vec<Argument>,
    },
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Load(_) => "load",
            Command::OutputFile(_) => "output-file",
            Command::CompareTo(_) => "compare-to",
            Command::OutputList(_) => "output-list",
            Command::Set { .. } => "set",
            Command::Eval => "eval",
            Command::Output => "output",
            Command::Tick => "tick",
            Command::Tock => "tock",
            Command::Echo(_) => "echo",
            Command::ClearEcho => "clear-echo",
            Command::Breakpoint { .. } => "breakpoint",
            Command::ClearBreakpoints => "clear-breakpoints",
            Command::BuiltinMethod { .. } => "built-in method",
            Command::Unsupported { .. } => "unsupported command",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    pub name: String,
    pub quoted: bool,
    pub span: Span,
}

impl FileName {
    /// The name without its final extension.
    pub fn stem(&self) -> &str {
        self.name
            .rsplit_once('.')
            .map_or(&self.name, |(stem, _)| stem)
    }

    pub fn extension(&self) -> Option<&str> {
        self.name.rsplit_once('.').map(|(_, ext)| ext)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argument {
    pub kind: ArgumentKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentKind {
    Word(String),
    String(String),
    Percent { format: char, body: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: Ident,
    /// `Some` for built-in state references `Name[]` / `Name[i]`.
    pub index: Option<VariableIndex>,
    pub span: Span,
}

impl Variable {
    /// Whether this is the reserved, read-only `time` variable.
    pub fn is_time(&self) -> bool {
        self.index.is_none() && self.name.name.eq_ignore_ascii_case("time")
    }
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name.name)?;
        match &self.index {
            Some(VariableIndex { value: Some(i), .. }) => write!(f, "[{i}]"),
            Some(VariableIndex { value: None, .. }) => f.write_str("[]"),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariableIndex {
    /// `None` for `[]`.
    pub value: Option<u64>,
    /// Span including the brackets.
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputItem {
    pub variable: Variable,
    /// The explicit format, or `None` for the default `%B1.1.1`.
    pub format: Option<OutputFormat>,
    pub span: Span,
}

impl OutputItem {
    pub fn effective_format(&self) -> OutputFormat {
        self.format.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputFormat {
    pub kind: FormatKind,
    pub pad_left: u32,
    pub len: u32,
    pub pad_right: u32,
    pub span: Span,
}

impl Default for OutputFormat {
    /// `%B1.1.1`
    fn default() -> Self {
        Self {
            kind: FormatKind::Binary,
            pad_left: 1,
            len: 1,
            pad_right: 1,
            span: Span::default(),
        }
    }
}

impl OutputFormat {
    /// Total column width: `pad_left + len + pad_right`.
    pub fn total_width(&self) -> u32 {
        self.pad_left + self.len + self.pad_right
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    Binary,
    Decimal,
    Hex,
    String,
}

impl FormatKind {
    pub fn from_letter(letter: char) -> Option<Self> {
        match letter.to_ascii_uppercase() {
            'B' => Some(FormatKind::Binary),
            'D' => Some(FormatKind::Decimal),
            'X' => Some(FormatKind::Hex),
            'S' => Some(FormatKind::String),
            _ => None,
        }
    }

    pub fn letter(self) -> char {
        match self {
            FormatKind::Binary => 'B',
            FormatKind::Decimal => 'D',
            FormatKind::Hex => 'X',
            FormatKind::String => 'S',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLiteral {
    pub value: String,
    /// Whether written in the `%S"..."` form.
    pub percent: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radix {
    Binary,
    Decimal,
    Hex,
}

impl Radix {
    pub fn base(self) -> u32 {
        match self {
            Radix::Binary => 2,
            Radix::Decimal => 10,
            Radix::Hex => 16,
        }
    }
}

/// An integer literal. Digits are validated by the parser, so
/// [`IntegerLiteral::digits`] is non-empty and valid for `radix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegerLiteral {
    pub radix: Radix,
    /// Only decimal literals may be negative.
    pub negative: bool,
    pub digits: String,
    pub span: Span,
}

/// Maximum digits for `%B` and `%X` literals.
pub const MAX_BINARY_DIGITS: usize = 16;
pub const MAX_HEX_DIGITS: usize = 4;

impl IntegerLiteral {
    /// The unsigned magnitude of the digits, if it fits in 64 bits.
    pub fn magnitude(&self) -> Option<u64> {
        u64::from_str_radix(&self.digits, self.radix.base()).ok()
    }

    /// Validates lexical limits: at most 16 binary or 4 hex digits; decimals
    /// within the baseline literal domain `-32768..=65535`.
    pub fn validate(&self) -> Result<(), String> {
        match self.radix {
            Radix::Binary if self.digits.len() > MAX_BINARY_DIGITS => Err(format!(
                "binary literals may have at most {MAX_BINARY_DIGITS} digits"
            )),
            Radix::Hex if self.digits.len() > MAX_HEX_DIGITS => Err(format!(
                "hexadecimal literals may have at most {MAX_HEX_DIGITS} digits"
            )),
            Radix::Decimal => {
                let value = self
                    .signed_decimal()
                    .ok_or("decimal literal is too large")?;
                if !(-32768..=65535).contains(&value) {
                    Err("decimal literal is outside the 16-bit range".to_string())
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    fn signed_decimal(&self) -> Option<i64> {
        let magnitude = i64::try_from(self.magnitude()?).ok()?;
        Some(if self.negative { -magnitude } else { magnitude })
    }

    /// Encodes the literal as a bit pattern for a writable destination of
    /// `width` bits (§4.1 of the test-script specification):
    ///
    /// - decimal: `-2^(w-1) ..= 2^w - 1` for `w < 16`, `-32768 ..= 32767` for
    ///   `w = 16`; negatives are encoded modulo `2^w`.
    /// - binary/hex: the unsigned pattern must fit in `w` bits; short patterns
    ///   are zero-extended.
    pub fn encode(&self, width: u32) -> Result<u64, String> {
        self.validate()?;
        let width = width.clamp(1, 64);
        let modulus_mask = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        match self.radix {
            Radix::Decimal => {
                let value = self
                    .signed_decimal()
                    .ok_or("decimal literal is too large")?;
                let (min, max) = if width == 16 {
                    (-32768, 32767)
                } else {
                    let w = width.min(62);
                    (-(1i64 << (w - 1)), (1i64 << w) - 1)
                };
                if value < min || value > max {
                    return Err(format!(
                        "{value} does not fit in {width} bit{} (allowed range {min}..={max})",
                        if width == 1 { "" } else { "s" }
                    ));
                }
                Ok((value as u64) & modulus_mask)
            }
            Radix::Binary | Radix::Hex => {
                let value = self.magnitude().ok_or("literal is too large")?;
                if value & !modulus_mask != 0 {
                    return Err(format!(
                        "pattern {} does not fit in {width} bit{}",
                        self,
                        if width == 1 { "" } else { "s" }
                    ));
                }
                Ok(value)
            }
        }
    }

    /// The value of the literal used as a condition or breakpoint constant:
    /// decimals are their signed value; 16-digit binary and 4-digit hex
    /// patterns are signed 16-bit values; shorter patterns are non-negative.
    pub fn constant_value(&self) -> Result<i64, String> {
        self.validate()?;
        match self.radix {
            Radix::Decimal => {
                let value = self
                    .signed_decimal()
                    .ok_or("decimal literal is too large")?;
                if !(-32768..=32767).contains(&value) {
                    return Err(format!("{value} is outside the signed 16-bit range"));
                }
                Ok(value)
            }
            Radix::Binary | Radix::Hex => {
                let value = self.magnitude().ok_or("literal is too large")? as i64;
                let full = match self.radix {
                    Radix::Binary => self.digits.len() == MAX_BINARY_DIGITS,
                    _ => self.digits.len() == MAX_HEX_DIGITS,
                };
                Ok(if full && value >= 0x8000 {
                    value - 0x10000
                } else {
                    value
                })
            }
        }
    }
}

impl fmt::Display for IntegerLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.radix {
            Radix::Binary => write!(f, "%B{}", self.digits),
            Radix::Hex => write!(f, "%X{}", self.digits),
            Radix::Decimal => write!(f, "{}{}", if self.negative { "-" } else { "" }, self.digits),
        }
    }
}
