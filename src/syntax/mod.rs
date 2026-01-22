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
    NormalizedElement, NormalizedPackage, NormalizedDefinition, NormalizedUsage,
    NormalizedImport, NormalizedAlias, NormalizedComment, NormalizedDefKind,
    NormalizedUsageKind, NormalizedRelationship, NormalizedRelKind,
    SysMLNormalizedIter, KerMLNormalizedIter,
};

#[cfg(test)]
mod tests;
