//! Input types for the Salsa database.

use std::sync::Arc;
use indexmap::IndexMap;
use crate::base::FileId;

/// The set of all source files in the workspace.
///
/// This is an input to the database — you set it explicitly,
/// and queries derive from it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceRoot {
    /// Map from FileId to file path (for display/error messages)
    files: IndexMap<FileId, Arc<str>>,
}

impl SourceRoot {
    /// Create a new empty source root.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file to the source root.
    pub fn insert(&mut self, file: FileId, path: impl Into<Arc<str>>) {
        self.files.insert(file, path.into());
    }

    /// Remove a file from the source root.
    pub fn remove(&mut self, file: FileId) -> Option<Arc<str>> {
        self.files.swap_remove(&file)
    }

    /// Get the path for a file.
    pub fn path(&self, file: FileId) -> Option<&str> {
        self.files.get(&file).map(|s| s.as_ref())
    }

    /// Check if a file exists in the source root.
    pub fn contains(&self, file: FileId) -> bool {
        self.files.contains_key(&file)
    }

    /// Iterate over all files.
    pub fn iter(&self) -> impl Iterator<Item = (FileId, &str)> + '_ {
        self.files.iter().map(|(&id, path)| (id, path.as_ref()))
    }

    /// Get the number of files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Check if the source root is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_root_basic() {
        let mut root = SourceRoot::new();
        
        let file1 = FileId::new(0);
        let file2 = FileId::new(1);
        
        root.insert(file1, "/path/to/a.sysml");
        root.insert(file2, "/path/to/b.sysml");
        
        assert_eq!(root.len(), 2);
        assert!(root.contains(file1));
        assert_eq!(root.path(file1), Some("/path/to/a.sysml"));
    }

    #[test]
    fn test_source_root_remove() {
        let mut root = SourceRoot::new();
        let file = FileId::new(0);
        
        root.insert(file, "/path/to/a.sysml");
        assert!(root.contains(file));
        
        root.remove(file);
        assert!(!root.contains(file));
    }
}
