//! Selection ranges — expanding selection regions.
//!
//! This module provides selection range extraction from the HIR SymbolIndex,
//! finding all symbols whose span contains the given position.

use crate::base::FileId;
use crate::hir::SymbolIndex;

/// A selection range with parent chain.
#[derive(Debug, Clone)]
pub struct SelectionRange {
    /// Start line (0-indexed)
    pub start_line: u32,
    /// Start column (0-indexed)
    pub start_col: u32,
    /// End line (0-indexed)
    pub end_line: u32,
    /// End column (0-indexed)
    pub end_col: u32,
}

/// Get selection ranges at a position.
///
/// Returns spans from innermost to outermost that contain the position.
/// Used for "Expand Selection" feature.
pub fn selection_ranges(
    index: &SymbolIndex,
    file: FileId,
    line: u32,
    col: u32,
) -> Vec<SelectionRange> {
    let mut ranges: Vec<SelectionRange> = index
        .symbols_in_file(file)
        .into_iter()
        .filter(|sym| {
            // Check if position is within symbol's span
            let after_start =
                line > sym.start_line || (line == sym.start_line && col >= sym.start_col);
            let before_end = line < sym.end_line || (line == sym.end_line && col <= sym.end_col);
            after_start && before_end
        })
        .map(|sym| SelectionRange {
            start_line: sym.start_line,
            start_col: sym.start_col,
            end_line: sym.end_line,
            end_col: sym.end_col,
        })
        .collect();

    // Sort by range size (smallest first for innermost)
    ranges.sort_by_key(range_size);

    // Deduplicate ranges with the same bounds
    ranges.dedup_by(|a, b| {
        a.start_line == b.start_line
            && a.start_col == b.start_col
            && a.end_line == b.end_line
            && a.end_col == b.end_col
    });

    ranges
}

/// Calculate a rough "size" of a range for sorting
fn range_size(range: &SelectionRange) -> u32 {
    let lines = range.end_line.saturating_sub(range.start_line);
    let cols = if lines == 0 {
        range.end_col.saturating_sub(range.start_col)
    } else {
        range.end_col + 100
    };
    lines * 100 + cols
}
