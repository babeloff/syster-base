//! Foundation types for the Syster toolchain.
//!
//! This module provides fundamental types used throughout the compiler:
//! - [`FileId`] - Interned file identifiers
//! - [`TextRange`], [`TextSize`] - Source positions  
//! - [`LineCol`], [`LineIndex`] - Line/column conversion
//! - [`Name`], [`Interner`] - String interning
//!
//! This module has NO dependencies on other syster modules.

mod file_id;
mod intern;
mod span;

pub use file_id::FileId;
pub use intern::{Name, Interner};
pub use span::{TextRange, TextSize, LineCol, LineIndex};

// Re-export text-size types for convenience
pub use text_size;
