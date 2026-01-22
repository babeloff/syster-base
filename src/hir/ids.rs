//! Semantic identifiers for definitions.

use std::fmt;

use crate::base::FileId;

/// A globally unique identifier for a definition.
///
/// Combines the file where the definition lives with a file-local ID.
/// This allows efficient per-file invalidation while still having
/// globally unique identifiers.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct DefId {
    /// The file containing this definition
    pub file: FileId,
    /// The local ID within the file
    pub local: LocalDefId,
}

impl DefId {
    /// Create a new DefId.
    #[inline]
    pub const fn new(file: FileId, local: LocalDefId) -> Self {
        Self { file, local }
    }
}

impl fmt::Debug for DefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DefId({:?}:{})", self.file, self.local.0)
    }
}

/// A file-local definition identifier.
///
/// These are assigned sequentially as definitions are discovered in a file.
/// They're stable across re-parses as long as the definition order doesn't change.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct LocalDefId(pub u32);

impl LocalDefId {
    /// Create a new LocalDefId.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw index.
    #[inline]
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for LocalDefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LocalDefId({})", self.0)
    }
}

impl From<u32> for LocalDefId {
    #[inline]
    fn from(id: u32) -> Self {
        Self(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_def_id_equality() {
        let file1 = FileId::new(1);
        let file2 = FileId::new(2);
        
        let a = DefId::new(file1, LocalDefId::new(0));
        let b = DefId::new(file1, LocalDefId::new(0));
        let c = DefId::new(file1, LocalDefId::new(1));
        let d = DefId::new(file2, LocalDefId::new(0));

        assert_eq!(a, b);
        assert_ne!(a, c); // different local
        assert_ne!(a, d); // different file
    }

    #[test]
    fn test_def_id_size() {
        // DefId should be 8 bytes (FileId + LocalDefId)
        assert_eq!(std::mem::size_of::<DefId>(), 8);
    }
}
