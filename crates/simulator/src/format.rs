//! Fixed-column `.out` formatting and exact `.cmp` comparison.

use n2t_core::tst::ast::{FormatKind, OutputFormat};

/// A value observed by a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A bit pattern of `width` bits.
    Int {
        bits: u64,
        width: u32,
    },
    Text(String),
}

impl Value {
    /// The numeric value: 16-bit values are signed, narrower ones unsigned.
    pub fn scalar(&self) -> Option<i64> {
        match *self {
            Value::Int { bits, width: 16 } => Some(bits as u16 as i16 as i64),
            Value::Int { bits, .. } => Some(bits as i64),
            Value::Text(_) => None,
        }
    }
}

/// `|` followed by each centred header field and `|`.
pub fn header<'a>(items: impl IntoIterator<Item = (&'a str, OutputFormat)>) -> String {
    let mut line = String::from("|");
    for (name, format) in items {
        let width = format.total_width() as usize;
        let name: String = name.chars().take(width).collect();
        let len = name.chars().count();
        let left = (width - len) / 2;
        line.push_str(&" ".repeat(left));
        line.push_str(&name);
        line.push_str(&" ".repeat(width - len - left));
        line.push('|');
    }
    line
}

/// `|` followed by each formatted field and `|`.
pub fn row<'a>(fields: impl IntoIterator<Item = (&'a Value, OutputFormat)>) -> String {
    let mut line = String::from("|");
    for (value, format) in fields {
        line.push_str(&field(value, format));
        line.push('|');
    }
    line
}

/// Formats one field: `pad_left` spaces, the value in `len` columns, then
/// `pad_right` spaces. Text longer than `len` keeps its rightmost characters.
pub fn field(value: &Value, format: OutputFormat) -> String {
    let (text, numeric) = match (format.kind, value) {
        (FormatKind::String, Value::Text(t)) => (t.clone(), false),
        (FormatKind::String, v) => (v.scalar().unwrap_or(0).to_string(), false),
        (_, Value::Text(t)) => (t.clone(), false),
        (FormatKind::Decimal, v) => (v.scalar().unwrap_or(0).to_string(), true),
        (FormatKind::Binary, Value::Int { bits, width }) => {
            let digits = (*width).max(16) as usize;
            (format!("{:0digits$b}", bits & mask(digits as u32)), true)
        }
        (FormatKind::Hex, Value::Int { bits, width }) => {
            let digits = (*width).max(16).div_ceil(4) as usize;
            (
                format!("{:0digits$x}", bits & mask(digits as u32 * 4)),
                true,
            )
        }
    };
    let len = format.len as usize;
    let chars: Vec<char> = text.chars().collect();
    let clipped: String = chars[chars.len().saturating_sub(len)..].iter().collect();
    let fill = " ".repeat(len - clipped.chars().count());
    let body = if numeric {
        format!("{fill}{clipped}")
    } else {
        format!("{clipped}{fill}")
    };
    format!(
        "{}{body}{}",
        " ".repeat(format.pad_left as usize),
        " ".repeat(format.pad_right as usize)
    )
}

fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1 << bits) - 1
    }
}

/// A comparison failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// One-based physical line of the expected file.
    pub line: usize,
    /// One-based column of the first difference, if both lines exist.
    pub column: Option<usize>,
    /// `None` when the expected file ended early.
    pub expected: Option<String>,
    /// `None` when expected lines were left over at the end.
    pub actual: Option<String>,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.expected, &self.actual) {
            (Some(e), Some(a)) => write!(
                f,
                "comparison failure at line {}{}:\n  expected: {e}\n  actual:   {a}",
                self.line,
                self.column
                    .map(|c| format!(", column {c}"))
                    .unwrap_or_default()
            ),
            (None, Some(a)) => write!(
                f,
                "comparison failure at line {}: the compare file ended, but the script produced:\n  actual:   {a}",
                self.line
            ),
            (Some(e), None) => write!(
                f,
                "comparison failure at line {}: the script ended, but the compare file expects more output:\n  expected: {e}",
                self.line
            ),
            (None, None) => write!(f, "comparison failure at line {}", self.line),
        }
    }
}

/// Line-by-line exact comparison where an expected `*` matches any one character.
#[derive(Debug, Clone)]
pub struct Comparator {
    lines: Vec<String>,
    next: usize,
}

impl Comparator {
    pub fn new(expected: &str) -> Self {
        let mut lines: Vec<String> = expected
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        // A trailing newline does not start another line.
        if expected.ends_with('\n') {
            lines.pop();
        }
        if expected.is_empty() {
            lines.clear();
        }
        Self { lines, next: 0 }
    }

    /// Number of lines compared so far.
    pub fn compared(&self) -> usize {
        self.next
    }

    pub fn compare(&mut self, actual: &str) -> Result<(), Mismatch> {
        let line = self.next + 1;
        let Some(expected) = self.lines.get(self.next) else {
            return Err(Mismatch {
                line,
                column: None,
                expected: None,
                actual: Some(actual.to_string()),
            });
        };
        self.next += 1;
        let e: Vec<char> = expected.chars().collect();
        let a: Vec<char> = actual.chars().collect();
        let first_difference = e
            .iter()
            .zip(&a)
            .position(|(e, a)| *e != '*' && e != a)
            .or_else(|| (e.len() != a.len()).then(|| e.len().min(a.len())));
        match first_difference {
            None => Ok(()),
            Some(column) => Err(Mismatch {
                line,
                column: Some(column + 1),
                expected: Some(expected.clone()),
                actual: Some(actual.to_string()),
            }),
        }
    }

    /// Fails if expected lines remain.
    pub fn finish(&self) -> Result<(), Mismatch> {
        match self.lines.get(self.next) {
            None => Ok(()),
            Some(expected) => Err(Mismatch {
                line: self.next + 1,
                column: None,
                expected: Some(expected.clone()),
                actual: None,
            }),
        }
    }
}
