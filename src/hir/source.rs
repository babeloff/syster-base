//! File set management for tracking source files.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::base::FileId;

/// Manages the mapping between file paths and FileIds.
///
/// This is the "file database" that assigns stable IDs to paths
/// and tracks file contents.
#[derive(Debug, Default)]
pub struct FileSet {
    inner: RwLock<FileSetInner>,
}

#[derive(Debug, Default)]
struct FileSetInner {
    /// Path → FileId mapping
    path_to_id: IndexMap<PathBuf, FileId>,
    /// FileId → Path mapping (reverse lookup)
    id_to_path: IndexMap<FileId, PathBuf>,
    /// FileId → Contents
    contents: IndexMap<FileId, Arc<str>>,
    /// Next FileId to assign
    next_id: u32,
}

impl FileSet {
    /// Create a new empty file set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a FileId for a path.
    ///
    /// If the path already has a FileId, returns it.
    /// Otherwise, assigns a new FileId.
    pub fn file_id(&self, path: &Path) -> FileId {
        // Fast path: read lock
        {
            let inner = self.inner.read();
            if let Some(&id) = inner.path_to_id.get(path) {
                return id;
            }
        }

        // Slow path: write lock
        let mut inner = self.inner.write();
        
        // Double-check
        if let Some(&id) = inner.path_to_id.get(path) {
            return id;
        }

        let id = FileId::new(inner.next_id);
        inner.next_id += 1;
        inner.path_to_id.insert(path.to_owned(), id);
        inner.id_to_path.insert(id, path.to_owned());
        id
    }

    /// Get the path for a FileId.
    pub fn path(&self, file: FileId) -> Option<PathBuf> {
        self.inner.read().id_to_path.get(&file).cloned()
    }

    /// Set the contents of a file.
    pub fn set_contents(&self, file: FileId, contents: impl Into<Arc<str>>) {
        self.inner.write().contents.insert(file, contents.into());
    }

    /// Get the contents of a file.
    pub fn contents(&self, file: FileId) -> Option<Arc<str>> {
        self.inner.read().contents.get(&file).cloned()
    }

    /// Remove a file from the set.
    pub fn remove(&self, file: FileId) {
        let mut inner = self.inner.write();
        if let Some(path) = inner.id_to_path.swap_remove(&file) {
            inner.path_to_id.swap_remove(&path);
        }
        inner.contents.swap_remove(&file);
    }

    /// Get the number of files.
    pub fn len(&self) -> usize {
        self.inner.read().path_to_id.len()
    }

    /// Check if the file set is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate over all file IDs.
    pub fn files(&self) -> Vec<FileId> {
        self.inner.read().id_to_path.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_set_id_assignment() {
        let files = FileSet::new();
        
        let id1 = files.file_id(Path::new("/a.sysml"));
        let id2 = files.file_id(Path::new("/b.sysml"));
        let id3 = files.file_id(Path::new("/a.sysml")); // same as id1
        
        assert_ne!(id1, id2);
        assert_eq!(id1, id3); // stable ID for same path
    }

    #[test]
    fn test_file_set_contents() {
        let files = FileSet::new();
        let id = files.file_id(Path::new("/test.sysml"));
        
        assert!(files.contents(id).is_none());
        
        files.set_contents(id, "part def Foo;");
        
        assert_eq!(files.contents(id).as_deref(), Some("part def Foo;"));
    }

    #[test]
    fn test_file_set_path_lookup() {
        let files = FileSet::new();
        let path = Path::new("/test.sysml");
        let id = files.file_id(path);
        
        assert_eq!(files.path(id).as_deref(), Some(path));
    }
}
