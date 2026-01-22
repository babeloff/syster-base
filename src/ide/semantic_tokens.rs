//! Semantic tokens — syntax highlighting based on semantic analysis.
//!
//! This module provides semantic token extraction directly from the HIR layer,
//! without depending on the legacy semantic layer.

use crate::base::FileId;
use crate::hir::{SymbolIndex, SymbolKind};

/// Token type for semantic highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Namespace,
    Type,
    Variable,
    Property,
    Keyword,
    Comment,
}

impl TokenType {
    /// Convert to LSP token type index.
    pub fn to_lsp_index(self) -> u32 {
        match self {
            TokenType::Namespace => 0,
            TokenType::Type => 1,
            TokenType::Variable => 2,
            TokenType::Property => 3,
            TokenType::Keyword => 4,
            TokenType::Comment => 5,
        }
    }
}

impl From<SymbolKind> for TokenType {
    fn from(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::Package => TokenType::Namespace,
            // All definition types
            SymbolKind::PartDef
            | SymbolKind::ItemDef
            | SymbolKind::ActionDef
            | SymbolKind::PortDef
            | SymbolKind::AttributeDef
            | SymbolKind::ConnectionDef
            | SymbolKind::InterfaceDef
            | SymbolKind::AllocationDef
            | SymbolKind::RequirementDef
            | SymbolKind::ConstraintDef
            | SymbolKind::StateDef
            | SymbolKind::CalculationDef
            | SymbolKind::UseCaseDef
            | SymbolKind::AnalysisCaseDef
            | SymbolKind::ConcernDef
            | SymbolKind::ViewDef
            | SymbolKind::ViewpointDef
            | SymbolKind::RenderingDef
            | SymbolKind::EnumerationDef => TokenType::Type,
            // All usage types
            SymbolKind::PartUsage
            | SymbolKind::ItemUsage
            | SymbolKind::ActionUsage
            | SymbolKind::PortUsage
            | SymbolKind::AttributeUsage
            | SymbolKind::ConnectionUsage
            | SymbolKind::InterfaceUsage
            | SymbolKind::AllocationUsage
            | SymbolKind::RequirementUsage
            | SymbolKind::ConstraintUsage
            | SymbolKind::StateUsage
            | SymbolKind::CalculationUsage
            | SymbolKind::ReferenceUsage
            | SymbolKind::OccurrenceUsage
            | SymbolKind::FlowUsage => TokenType::Property,
            // Other types
            SymbolKind::Alias => TokenType::Variable,
            SymbolKind::Import => TokenType::Namespace,
            SymbolKind::Comment => TokenType::Comment,
            SymbolKind::Dependency => TokenType::Variable,
            SymbolKind::Other => TokenType::Variable,
        }
    }
}

/// A semantic token for syntax highlighting.
#[derive(Debug, Clone)]
pub struct SemanticToken {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed)
    pub col: u32,
    /// Length of the token in characters
    pub length: u32,
    /// The token type
    pub token_type: TokenType,
}

/// Get semantic tokens for a file.
///
/// Uses the symbol index to generate tokens for symbol definitions and type references.
///
/// # Arguments
///
/// * `index` - The symbol index containing all symbols
/// * `file` - The file to get tokens for
///
/// # Returns
///
/// Vector of semantic tokens sorted by position.
pub fn semantic_tokens(index: &SymbolIndex, file: FileId) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();

    // Add tokens for all symbols in this file
    for symbol in index.symbols_in_file(file) {
        // Token for the symbol name itself
        let name_len = symbol.name.len() as u32;
        tokens.push(SemanticToken {
            line: symbol.start_line,
            col: symbol.start_col,
            length: name_len,
            token_type: TokenType::from(symbol.kind),
        });

        // Tokens for type references (the types in `:>` or `:` relationships)
        for type_ref_kind in &symbol.type_refs {
            for type_ref in type_ref_kind.as_refs() {
                tokens.push(SemanticToken {
                    line: type_ref.start_line,
                    col: type_ref.start_col,
                    length: (type_ref.end_col - type_ref.start_col).max(1),
                    token_type: TokenType::Type,
                });
            }
        }
    }

    // Sort tokens by position (line, then column)
    tokens.sort_by_key(|t| (t.line, t.col));

    tokens
}
