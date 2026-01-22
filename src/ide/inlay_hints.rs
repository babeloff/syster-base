//! Inlay hints — inline type and parameter annotations.
//!
//! This module provides inlay hint extraction directly from the HIR layer,
//! without depending on the legacy semantic layer.

use crate::base::FileId;
use crate::hir::SymbolIndex;

/// Kind of inlay hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlayHintKind {
    /// Type annotation hint (e.g., `: Real`)
    Type,
    /// Parameter name hint (e.g., `width:`)
    Parameter,
}

/// An inlay hint to display in the editor.
#[derive(Debug, Clone)]
pub struct InlayHint {
    /// Line where the hint should appear (0-indexed)
    pub line: u32,
    /// Column where the hint should appear (0-indexed)
    pub col: u32,
    /// The text to display
    pub label: String,
    /// The kind of hint
    pub kind: InlayHintKind,
    /// Whether to add padding before the hint
    pub padding_left: bool,
    /// Whether to add padding after the hint
    pub padding_right: bool,
}

/// Get inlay hints for a file.
///
/// Returns type hints for symbols that have explicit type annotations.
/// Currently shows the first supertype for usages.
///
/// # Arguments
///
/// * `index` - The symbol index containing all symbols
/// * `file` - The file to get hints for
/// * `range` - Optional range to filter hints (start_line, start_col, end_line, end_col)
///
/// # Returns
///
/// Vector of inlay hints within the specified range.
pub fn inlay_hints(
    index: &SymbolIndex,
    file: FileId,
    range: Option<(u32, u32, u32, u32)>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    for symbol in index.symbols_in_file(file) {
        // Skip if outside the requested range
        if let Some((start_line, start_col, end_line, end_col)) = range {
            if symbol.start_line < start_line
                || symbol.end_line > end_line
                || (symbol.start_line == start_line && symbol.start_col < start_col)
                || (symbol.end_line == end_line && symbol.end_col > end_col)
            {
                continue;
            }
        }

        // Only show type hints for usages with explicit types
        if symbol.kind.is_usage() && !symbol.supertypes.is_empty() {
            // Show the primary type (first supertype, which is typically the typed_by)
            let type_name = &symbol.supertypes[0];
            
            // Position hint after the symbol name
            let hint_col = symbol.start_col + symbol.name.len() as u32;
            
            hints.push(InlayHint {
                line: symbol.start_line,
                col: hint_col,
                label: format!(": {type_name}"),
                kind: InlayHintKind::Type,
                padding_left: false,
                padding_right: true,
            });
        }
    }

    hints
}
