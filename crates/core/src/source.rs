//! Source locations shared by every front-end in this crate.

use std::fmt;

/// Represents the byte offsets within the original parsed UTF-8 string
/// a given token or syntax node represents. `end` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// An empty span positioned at `offset`.
    pub const fn empty(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The smallest span covering both `self` and `other`.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Whether `offset` lies within this span. The end offset is included so
    /// that a cursor placed directly after a token still "touches" it.
    pub const fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }

    /// The text this span covers within `source`.
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// A zero-based line/column position. The unit of `column` depends on the
/// method that produced it (bytes or UTF-16 code units).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// Converts between byte offsets and line/column positions.
///
/// Lines are terminated by `\n`; a preceding `\r` is treated as part of the
/// line terminator for column purposes only when it is the final character.
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            len: text.len(),
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The byte offset at which `line` starts, if it exists.
    pub fn line_start(&self, line: u32) -> Option<usize> {
        self.line_starts.get(line as usize).copied()
    }

    fn line_of(&self, offset: usize) -> usize {
        let offset = offset.min(self.len);
        match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next - 1,
        }
    }

    /// Position with the column measured in bytes.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        Position {
            line: line as u32,
            column: (offset - self.line_starts[line]) as u32,
        }
    }

    /// Position with the column measured in UTF-16 code units, as used by the
    /// Language Server Protocol. `text` must be the text this index was built from.
    pub fn position_utf16(&self, text: &str, offset: usize) -> Position {
        let offset = floor_char_boundary(text, offset.min(self.len));
        let line = self.line_of(offset);
        let start = self.line_starts[line];
        let column = text[start..offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        Position {
            line: line as u32,
            column: column as u32,
        }
    }

    /// One-based line and column (bytes + 1), for human-readable diagnostics.
    pub fn line_col_one_based(&self, text: &str, offset: usize) -> (usize, usize) {
        let offset = floor_char_boundary(text, offset.min(self.len));
        let line = self.line_of(offset);
        let start = self.line_starts[line];
        (line + 1, text[start..offset].chars().count() + 1)
    }

    /// Converts a byte-column position back into a byte offset.
    pub fn offset(&self, position: Position) -> Option<usize> {
        let start = self.line_start(position.line)?;
        Some((start + position.column as usize).min(self.len))
    }

    /// Converts a UTF-16 position (LSP) back into a byte offset. Columns past
    /// the end of the line clamp to the end of the line.
    pub fn offset_utf16(&self, text: &str, position: Position) -> Option<usize> {
        let start = self.line_start(position.line)?;
        let end = self
            .line_starts
            .get(position.line as usize + 1)
            .copied()
            .unwrap_or(self.len);
        let mut units = 0usize;
        for (i, c) in text[start..end].char_indices() {
            if units >= position.column as usize || c == '\n' {
                return Some(start + i);
            }
            units += c.len_utf16();
        }
        Some(end)
    }
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Strips an optional UTF-8 byte order mark, returning the number of bytes skipped.
pub(crate) fn bom_len(text: &str) -> usize {
    if text.starts_with('\u{FEFF}') { 3 } else { 0 }
}
