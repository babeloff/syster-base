//! Source text positions and ranges.

use std::fmt;

// Re-export from text-size for compatibility
pub use text_size::TextRange;
pub use text_size::TextSize;

/// A line and column position in source text.
///
/// Both line and column are 0-indexed internally, but displayed as 1-indexed.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct LineCol {
    /// 0-indexed line number
    pub line: u32,
    /// 0-indexed column (in UTF-8 bytes, not characters)
    pub col: u32,
}

impl LineCol {
    /// Create a new LineCol position.
    #[inline]
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    /// Create from 1-indexed line and column (as displayed to users).
    #[inline]
    pub const fn from_one_indexed(line: u32, col: u32) -> Self {
        Self {
            line: line.saturating_sub(1),
            col: col.saturating_sub(1),
        }
    }

    /// Get 1-indexed line number (for display).
    #[inline]
    pub const fn line_one_indexed(self) -> u32 {
        self.line + 1
    }

    /// Get 1-indexed column number (for display).
    #[inline]
    pub const fn col_one_indexed(self) -> u32 {
        self.col + 1
    }
}

impl fmt::Debug for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line_one_indexed(), self.col_one_indexed())
    }
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line_one_indexed(), self.col_one_indexed())
    }
}

/// Index for converting between byte offsets and line/column positions.
#[derive(Clone, Debug)]
pub struct LineIndex {
    /// Byte offset of the start of each line
    line_starts: Vec<TextSize>,
}

impl LineIndex {
    /// Build a line index from source text.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![TextSize::from(0)];
        
        for (offset, c) in text.char_indices() {
            if c == '\n' {
                line_starts.push(TextSize::from((offset + 1) as u32));
            }
        }
        
        Self { line_starts }
    }

    /// Convert a byte offset to a line/column position.
    pub fn line_col(&self, offset: TextSize) -> LineCol {
        let line = self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        
        let line_start = self.line_starts[line];
        let col = offset - line_start;
        
        LineCol {
            line: line as u32,
            col: col.into(),
        }
    }

    /// Convert a line/column position to a byte offset.
    pub fn offset(&self, line_col: LineCol) -> Option<TextSize> {
        let line_start = self.line_starts.get(line_col.line as usize)?;
        Some(*line_start + TextSize::from(line_col.col))
    }

    /// Get the number of lines.
    pub fn len(&self) -> usize {
        self.line_starts.len()
    }

    /// Check if there are no lines (empty file).
    pub fn is_empty(&self) -> bool {
        self.line_starts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_col_display() {
        let pos = LineCol::new(0, 0);
        assert_eq!(format!("{}", pos), "1:1");

        let pos = LineCol::new(5, 10);
        assert_eq!(format!("{}", pos), "6:11");
    }

    #[test]
    fn test_line_col_from_one_indexed() {
        let pos = LineCol::from_one_indexed(1, 1);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.col, 0);
    }

    #[test]
    fn test_line_index_single_line() {
        let index = LineIndex::new("hello world");
        
        assert_eq!(index.line_col(TextSize::from(0)), LineCol::new(0, 0));
        assert_eq!(index.line_col(TextSize::from(5)), LineCol::new(0, 5));
    }

    #[test]
    fn test_line_index_multi_line() {
        let index = LineIndex::new("hello\nworld\n!");
        
        assert_eq!(index.line_col(TextSize::from(0)), LineCol::new(0, 0));
        assert_eq!(index.line_col(TextSize::from(5)), LineCol::new(0, 5));
        assert_eq!(index.line_col(TextSize::from(6)), LineCol::new(1, 0));
        assert_eq!(index.line_col(TextSize::from(11)), LineCol::new(1, 5));
        assert_eq!(index.line_col(TextSize::from(12)), LineCol::new(2, 0));
    }

    #[test]
    fn test_line_index_offset() {
        let index = LineIndex::new("hello\nworld");
        
        assert_eq!(index.offset(LineCol::new(0, 0)), Some(TextSize::from(0)));
        assert_eq!(index.offset(LineCol::new(1, 0)), Some(TextSize::from(6)));
        assert_eq!(index.offset(LineCol::new(1, 3)), Some(TextSize::from(9)));
    }
}
