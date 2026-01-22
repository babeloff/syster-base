//! IDE features — High-level APIs for LSP handlers.
//!
//! This module provides the interface between the semantic model (HIR)
//! and the LSP server. Each function corresponds to an LSP request.
//!
//! ## Design Principles
//!
//! 1. **Pure functions**: Take data in, return data out
//! 2. **No LSP types**: Uses our own types, converted at LSP boundary
//! 3. **Composable**: Built on top of HIR queries
//!
//! ## Usage
//!
//! The recommended way to use this module is through `AnalysisHost`:
//!
//! ```ignore
//! use syster::ide::AnalysisHost;
//!
//! let mut host = AnalysisHost::new();
//! host.set_file_content("test.sysml", "package Test {}");
//!
//! let analysis = host.analysis();
//! let symbols = analysis.document_symbols(file_id);
//! ```

mod analysis;
mod goto;
mod hover;
mod completion;
mod references;
mod symbols;
mod document_links;
mod folding;
mod selection;
mod inlay_hints;
mod semantic_tokens;

pub use analysis::{AnalysisHost, Analysis};
pub use goto::{goto_definition, GotoResult, GotoTarget};
pub use hover::{hover, HoverResult};
pub use completion::{completions, CompletionItem, CompletionKind};
pub use references::{find_references, ReferenceResult, Reference};
pub use symbols::{workspace_symbols, document_symbols, SymbolInfo};
pub use document_links::{document_links, DocumentLink};
pub use folding::{folding_ranges, FoldingRange};
pub use selection::{selection_ranges, SelectionRange};
pub use inlay_hints::{inlay_hints, InlayHint, InlayHintKind};
pub use semantic_tokens::{semantic_tokens, SemanticToken, TokenType};
