//! File identifiers for tracking source files.

use std::fmt;

/// An interned identifier for a source file.
///
/// `FileId` is a lightweight handle (just a u32) that uniquely identifies
/// a file within the workspace. The actual path is stored elsewhere.
///
/// Using `FileId` instead of `PathBuf` throughout the codebase:
/// - Makes comparisons O(1) instead of O(n)
/// - Reduces memory usage (4 bytes vs ~24+ bytes)
/// - Enables cheap copying and hashing
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct FileId(pub u32);

impl FileId {
    /// Create a new FileId from a raw index.
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

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "file#{}", self.0)
    }
}

impl From<u32> for FileId {
    #[inline]
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<FileId> for u32 {
    #[inline]
    fn from(id: FileId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_id_equality() {
        let a = FileId::new(1);
        let b = FileId::new(1);
        let c = FileId::new(2);

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_file_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(FileId::new(1));
        set.insert(FileId::new(2));
        set.insert(FileId::new(1)); // duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_file_id_size() {
        assert_eq!(std::mem::size_of::<FileId>(), 4);
    }
}
