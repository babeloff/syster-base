//! Diagnostics — Semantic error reporting.
//!
//! This module provides diagnostic types for semantic analysis errors
//! and warnings. It integrates with the symbol index and resolver.

use std::sync::Arc;

use crate::base::FileId;
use super::symbols::{HirSymbol, SymbolKind};
use super::resolve::{SymbolIndex, Resolver, ResolveResult};

// ============================================================================
// DIAGNOSTIC TYPES
// ============================================================================

/// Severity level of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    /// Convert to LSP severity number.
    pub fn to_lsp(&self) -> u32 {
        match self {
            Severity::Error => 1,
            Severity::Warning => 2,
            Severity::Info => 3,
            Severity::Hint => 4,
        }
    }
}

/// A diagnostic message with location.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// The file containing this diagnostic.
    pub file: FileId,
    /// Start line (0-indexed).
    pub start_line: u32,
    /// Start column (0-indexed).
    pub start_col: u32,
    /// End line (0-indexed).
    pub end_line: u32,
    /// End column (0-indexed).
    pub end_col: u32,
    /// Severity level.
    pub severity: Severity,
    /// Error/warning code (e.g., "E0001").
    pub code: Option<Arc<str>>,
    /// The diagnostic message.
    pub message: Arc<str>,
    /// Optional related information.
    pub related: Vec<RelatedInfo>,
}

/// Related information for a diagnostic.
#[derive(Clone, Debug)]
pub struct RelatedInfo {
    /// The file containing this info.
    pub file: FileId,
    /// Line number.
    pub line: u32,
    /// Column number.
    pub col: u32,
    /// The message.
    pub message: Arc<str>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(file: FileId, line: u32, col: u32, message: impl Into<Arc<str>>) -> Self {
        Self {
            file,
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            severity: Severity::Error,
            code: None,
            message: message.into(),
            related: Vec::new(),
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(file: FileId, line: u32, col: u32, message: impl Into<Arc<str>>) -> Self {
        Self {
            file,
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col,
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            related: Vec::new(),
        }
    }

    /// Set the span (range) for this diagnostic.
    pub fn with_span(mut self, end_line: u32, end_col: u32) -> Self {
        self.end_line = end_line;
        self.end_col = end_col;
        self
    }

    /// Set the error code.
    pub fn with_code(mut self, code: impl Into<Arc<str>>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Add related information.
    pub fn with_related(mut self, info: RelatedInfo) -> Self {
        self.related.push(info);
        self
    }
}

// ============================================================================
// DIAGNOSTIC CODES
// ============================================================================

/// Standard diagnostic codes for semantic errors.
pub mod codes {
    /// Undefined reference (name not found).
    pub const UNDEFINED_REFERENCE: &str = "E0001";
    /// Ambiguous reference (multiple candidates).
    pub const AMBIGUOUS_REFERENCE: &str = "E0002";
    /// Type mismatch.
    pub const TYPE_MISMATCH: &str = "E0003";
    /// Duplicate definition.
    pub const DUPLICATE_DEFINITION: &str = "E0004";
    /// Missing required element.
    pub const MISSING_REQUIRED: &str = "E0005";
    /// Invalid specialization.
    pub const INVALID_SPECIALIZATION: &str = "E0006";
    /// Circular dependency.
    pub const CIRCULAR_DEPENDENCY: &str = "E0007";
    
    /// Unused symbol.
    pub const UNUSED_SYMBOL: &str = "W0001";
    /// Deprecated usage.
    pub const DEPRECATED: &str = "W0002";
    /// Naming convention violation.
    pub const NAMING_CONVENTION: &str = "W0003";
}

// ============================================================================
// DIAGNOSTIC COLLECTOR
// ============================================================================

/// Collects diagnostics during semantic analysis.
#[derive(Clone, Debug, Default)]
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a diagnostic.
    pub fn add(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Add an undefined reference error.
    pub fn undefined_reference(&mut self, file: FileId, symbol: &HirSymbol, name: &str) {
        self.add(
            Diagnostic::error(
                file,
                symbol.start_line,
                symbol.start_col,
                format!("undefined reference: '{}'", name),
            )
            .with_span(symbol.end_line, symbol.end_col)
            .with_code(codes::UNDEFINED_REFERENCE),
        );
    }

    /// Add an ambiguous reference error.
    pub fn ambiguous_reference(&mut self, file: FileId, symbol: &HirSymbol, name: &str, candidates: &[HirSymbol]) {
        let candidate_names: Vec<_> = candidates.iter().map(|c| c.qualified_name.as_ref()).collect();
        let mut diag = Diagnostic::error(
            file,
            symbol.start_line,
            symbol.start_col,
            format!("ambiguous reference: '{}' could be: {}", name, candidate_names.join(", ")),
        )
        .with_span(symbol.end_line, symbol.end_col)
        .with_code(codes::AMBIGUOUS_REFERENCE);

        // Add related info for each candidate
        for candidate in candidates {
            diag = diag.with_related(RelatedInfo {
                file: candidate.file,
                line: candidate.start_line,
                col: candidate.start_col,
                message: Arc::from(format!("candidate: {}", candidate.qualified_name)),
            });
        }

        self.add(diag);
    }

    /// Add a duplicate definition error.
    pub fn duplicate_definition(&mut self, file: FileId, symbol: &HirSymbol, existing: &HirSymbol) {
        self.add(
            Diagnostic::error(
                file,
                symbol.start_line,
                symbol.start_col,
                format!("duplicate definition: '{}' is already defined", symbol.name),
            )
            .with_span(symbol.end_line, symbol.end_col)
            .with_code(codes::DUPLICATE_DEFINITION)
            .with_related(RelatedInfo {
                file: existing.file,
                line: existing.start_line,
                col: existing.start_col,
                message: Arc::from(format!("previous definition of '{}'", existing.name)),
            }),
        );
    }

    /// Add a type mismatch error.
    pub fn type_mismatch(&mut self, file: FileId, symbol: &HirSymbol, expected: &str, found: &str) {
        self.add(
            Diagnostic::error(
                file,
                symbol.start_line,
                symbol.start_col,
                format!("type mismatch: expected '{}', found '{}'", expected, found),
            )
            .with_span(symbol.end_line, symbol.end_col)
            .with_code(codes::TYPE_MISMATCH),
        );
    }

    /// Add an unused symbol warning.
    pub fn unused_symbol(&mut self, symbol: &HirSymbol) {
        self.add(
            Diagnostic::warning(
                symbol.file,
                symbol.start_line,
                symbol.start_col,
                format!("unused {}: '{}'", symbol.kind.display(), symbol.name),
            )
            .with_span(symbol.end_line, symbol.end_col)
            .with_code(codes::UNUSED_SYMBOL),
        );
    }

    /// Get all diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Get diagnostics for a specific file.
    pub fn diagnostics_for_file(&self, file: FileId) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.file == file).collect()
    }

    /// Get the number of errors.
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
    }

    /// Get the number of warnings.
    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Warning).count()
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    /// Take all diagnostics, leaving the collector empty.
    pub fn take(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Clear all diagnostics.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }
}

// ============================================================================
// SEMANTIC CHECKER
// ============================================================================

/// Performs semantic checks on symbols using the resolver.
pub struct SemanticChecker<'a> {
    index: &'a SymbolIndex,
    collector: DiagnosticCollector,
}

impl<'a> SemanticChecker<'a> {
    /// Create a new semantic checker.
    pub fn new(index: &'a SymbolIndex) -> Self {
        Self {
            index,
            collector: DiagnosticCollector::new(),
        }
    }

    /// Check all symbols in a file.
    pub fn check_file(&mut self, file: FileId) {
        let symbols = self.index.symbols_in_file(file);
        
        for symbol in symbols {
            self.check_symbol(symbol);
        }
    }

    /// Check a single symbol.
    fn check_symbol(&mut self, symbol: &HirSymbol) {
        // Check type references (supertypes)
        for supertype in &symbol.supertypes {
            self.check_reference(symbol, supertype);
        }
    }

    /// Check a reference resolves correctly.
    fn check_reference(&mut self, symbol: &HirSymbol, name: &str) {
        // Build resolver with appropriate scope
        let scope = self.extract_scope(&symbol.qualified_name);
        let resolver = Resolver::new(self.index).with_scope(scope);

        match resolver.resolve_type(name) {
            ResolveResult::Found(_) => {
                // Reference resolves successfully
            }
            ResolveResult::Ambiguous(candidates) => {
                self.collector.ambiguous_reference(symbol.file, symbol, name, &candidates);
            }
            ResolveResult::NotFound => {
                self.collector.undefined_reference(symbol.file, symbol, name);
            }
        }
    }

    /// Extract scope from a qualified name.
    fn extract_scope(&self, qualified_name: &str) -> String {
        if let Some(pos) = qualified_name.rfind("::") {
            qualified_name[..pos].to_string()
        } else {
            String::new()
        }
    }

    /// Get the collected diagnostics.
    pub fn finish(self) -> Vec<Diagnostic> {
        self.collector.diagnostics.into_iter().collect()
    }
}

/// Check a file and return diagnostics.
pub fn check_file(index: &SymbolIndex, file: FileId) -> Vec<Diagnostic> {
    let mut checker = SemanticChecker::new(index);
    checker.check_file(file);
    checker.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(name: &str, qualified: &str, kind: SymbolKind, file: u32) -> HirSymbol {
        HirSymbol {
            name: Arc::from(name),
            short_name: None,
            qualified_name: Arc::from(qualified),
            kind,
            file: FileId::new(file),
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            doc: None,
            supertypes: Vec::new(),
            type_refs: Vec::new(),
            is_public: false,
        }
    }

    #[test]
    fn test_diagnostic_error() {
        let diag = Diagnostic::error(FileId::new(0), 10, 5, "test error");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.start_line, 10);
        assert_eq!(diag.start_col, 5);
    }

    #[test]
    fn test_diagnostic_with_code() {
        let diag = Diagnostic::error(FileId::new(0), 0, 0, "test")
            .with_code(codes::UNDEFINED_REFERENCE);
        assert_eq!(diag.code.as_deref(), Some("E0001"));
    }

    #[test]
    fn test_collector_counts() {
        let mut collector = DiagnosticCollector::new();
        collector.add(Diagnostic::error(FileId::new(0), 0, 0, "error 1"));
        collector.add(Diagnostic::error(FileId::new(0), 0, 0, "error 2"));
        collector.add(Diagnostic::warning(FileId::new(0), 0, 0, "warning 1"));

        assert_eq!(collector.error_count(), 2);
        assert_eq!(collector.warning_count(), 1);
        assert!(collector.has_errors());
    }

    #[test]
    fn test_collector_by_file() {
        let mut collector = DiagnosticCollector::new();
        collector.add(Diagnostic::error(FileId::new(0), 0, 0, "file 0"));
        collector.add(Diagnostic::error(FileId::new(1), 0, 0, "file 1"));
        collector.add(Diagnostic::error(FileId::new(0), 0, 0, "file 0 again"));

        let file0_diags = collector.diagnostics_for_file(FileId::new(0));
        assert_eq!(file0_diags.len(), 2);

        let file1_diags = collector.diagnostics_for_file(FileId::new(1));
        assert_eq!(file1_diags.len(), 1);
    }

    #[test]
    fn test_severity_to_lsp() {
        assert_eq!(Severity::Error.to_lsp(), 1);
        assert_eq!(Severity::Warning.to_lsp(), 2);
        assert_eq!(Severity::Info.to_lsp(), 3);
        assert_eq!(Severity::Hint.to_lsp(), 4);
    }

    #[test]
    fn test_semantic_checker_undefined_reference() {
        let mut index = SymbolIndex::new();
        
        // Add a symbol that references a non-existent type
        let mut symbol = make_symbol("wheel", "Vehicle::wheel", SymbolKind::PartUsage, 0);
        symbol.supertypes = vec![Arc::from("NonExistent")];
        
        index.add_file(FileId::new(0), vec![symbol]);
        
        let diagnostics = check_file(&index, FileId::new(0));
        
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("undefined reference"));
    }

    #[test]
    fn test_semantic_checker_valid_reference() {
        let mut index = SymbolIndex::new();
        
        // Add the type definition
        let wheel_def = make_symbol("Wheel", "Wheel", SymbolKind::PartDef, 0);
        
        // Add a symbol that references the type
        let mut wheel_usage = make_symbol("wheel", "Vehicle::wheel", SymbolKind::PartUsage, 0);
        wheel_usage.supertypes = vec![Arc::from("Wheel")];
        
        index.add_file(FileId::new(0), vec![wheel_def, wheel_usage]);
        
        let diagnostics = check_file(&index, FileId::new(0));
        
        // Should have no errors - reference resolves
        assert_eq!(diagnostics.len(), 0);
    }
}
