// Syntax definitions for supported languages
pub mod file;
pub mod formatter;
pub mod kerml;
pub mod normalized;
pub mod parser;
pub mod sysml;

pub use file::SyntaxFile;
pub use formatter::{FormatOptions, format_async};
pub use normalized::{
    KerMLNormalizedIter, NormalizedAlias, NormalizedComment, NormalizedDefKind,
    NormalizedDefinition, NormalizedElement, NormalizedImport, NormalizedPackage,
    NormalizedRelKind, NormalizedRelationship, NormalizedUsage, NormalizedUsageKind,
    SysMLNormalizedIter,
};

#[cfg(test)]
mod tests;
