//! # syster-base
//!
//! Core library for SysML v2 and KerML parsing, AST, and semantic analysis.
//!
//! ## Module Structure (dependency order)
//!
//! ```text
//! ide     → IDE features (completion, hover, goto-def)
//!   ↓
//! hir     → Semantic model with Salsa queries  
//!   ↓
//! ast     → Typed syntax tree wrappers
//!   ↓
//! parser  → Lexer + parser (pest for now, hand-written later)
//!   ↓
//! base    → Primitives (FileId, Span, Name interning)
//! ```
//!
//! ## Legacy Modules (being replaced)
//!
//! - `core` - old primitives → moving to `base`
//! - `syntax` - old AST → moving to `ast`  
//! - `semantic` - old analysis → moving to `hir`
//! - `project` - workspace management → moving to `hir`

// ============================================================================
// NEW ARCHITECTURE (Phase 1)
// ============================================================================

/// Foundation types: FileId, Span, Name interning
pub mod base;

/// High-level IR: Salsa-based semantic model
pub mod hir;

/// IDE features: completion, hover, goto-definition, find-references
pub mod ide;

// Placeholder modules (to be implemented)
// pub mod parser2;  // New hand-written parser
// pub mod ast2;     // New typed syntax wrappers

// ============================================================================
// LEGACY MODULES (to be deprecated)
// ============================================================================

pub mod core;
pub mod parser;
pub mod project;
pub mod syntax;

// Re-export commonly needed items
pub use parser::keywords;

// Re-export new foundation types
pub use base::{FileId, Interner, LineCol, LineIndex, Name, TextRange, TextSize};
