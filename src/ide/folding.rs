//! Folding ranges — collapsible code regions.
//!
//! This module provides folding range extraction from the HIR SymbolIndex,
//! finding all symbols that span multiple lines.

use crate::base::FileId;
use crate::hir::{SymbolIndex, SymbolKind};

/// A folding range with position information.
#[derive(Debug, Clone)]
pub struct FoldingRange {
    /// Start line (0-indexed)
    pub start_line: u32,
    /// Start column (0-indexed)
    pub start_col: u32,
    /// End line (0-indexed)
    pub end_line: u32,
    /// End column (0-indexed)
    pub end_col: u32,
    /// Whether this is a comment region
    pub is_comment: bool,
}

/// Get folding ranges for a file.
///
/// Returns all collapsible regions (definitions, blocks, comments).
pub fn folding_ranges(index: &SymbolIndex, file: FileId) -> Vec<FoldingRange> {
    let mut ranges: Vec<FoldingRange> = index
        .symbols_in_file(file)
        .into_iter()
        .filter(|sym| sym.end_line > sym.start_line) // Only multiline symbols
        .map(|sym| FoldingRange {
            start_line: sym.start_line,
            start_col: sym.start_col,
            end_line: sym.end_line,
            end_col: sym.end_col,
            is_comment: sym.kind == SymbolKind::Comment,
        })
        .collect();

    // Sort by start line
    ranges.sort_by_key(|r| r.start_line);

    ranges
}
