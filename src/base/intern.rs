//! String interning for identifiers and paths.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::fmt;

/// An interned identifier name.
///
/// `Name` is a lightweight handle (just a u32) that represents an identifier
/// string. The actual string is stored in an [`Interner`].
///
/// Benefits:
/// - O(1) equality comparison
/// - 4 bytes storage vs variable-length string
/// - Cheap to copy and hash
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Name(u32);

impl Name {
    /// Create a Name from a raw index (used internally).
    #[inline]
    pub(crate) const fn from_raw(index: u32) -> Self {
        Self(index)
    }

    /// Get the raw index.
    #[inline]
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Name({})", self.0)
    }
}

/// String interner for deduplicating identifier strings.
///
/// Thread-safe via internal locking.
#[derive(Default)]
pub struct Interner {
    inner: RwLock<InternerInner>,
}

#[derive(Default)]
struct InternerInner {
    /// Map from string to index
    map: FxHashMap<SmolStr, u32>,
    /// Storage of all interned strings
    strings: Vec<SmolStr>,
}

impl Interner {
    /// Create a new empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a string, returning a `Name` handle.
    ///
    /// If the string has been interned before, returns the existing `Name`.
    pub fn intern(&self, s: &str) -> Name {
        // Fast path: check if already interned (read lock)
        {
            let inner = self.inner.read();
            if let Some(&index) = inner.map.get(s) {
                return Name::from_raw(index);
            }
        }

        // Slow path: need to insert (write lock)
        let mut inner = self.inner.write();
        
        // Double-check after acquiring write lock
        if let Some(&index) = inner.map.get(s) {
            return Name::from_raw(index);
        }

        let smol = SmolStr::new(s);
        let index = inner.strings.len() as u32;
        inner.strings.push(smol.clone());
        inner.map.insert(smol, index);
        
        Name::from_raw(index)
    }

    /// Look up the string for a `Name`.
    ///
    /// Returns `None` if the `Name` was created by a different interner.
    pub fn lookup(&self, name: Name) -> Option<SmolStr> {
        let inner = self.inner.read();
        inner.strings.get(name.0 as usize).cloned()
    }

    /// Look up the string for a `Name`, returning a reference.
    ///
    /// # Panics
    /// Panics if the `Name` was not created by this interner.
    pub fn get(&self, name: Name) -> SmolStr {
        self.lookup(name).expect("Name not found in interner")
    }

    /// Get the number of interned strings.
    pub fn len(&self) -> usize {
        self.inner.read().strings.len()
    }

    /// Check if the interner is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Debug for Interner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.read();
        f.debug_struct("Interner")
            .field("count", &inner.strings.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_same_string() {
        let interner = Interner::new();
        
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        
        assert_eq!(a, b);
        assert_eq!(interner.len(), 1);
    }

    #[test]
    fn test_intern_different_strings() {
        let interner = Interner::new();
        
        let a = interner.intern("hello");
        let b = interner.intern("world");
        
        assert_ne!(a, b);
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn test_lookup() {
        let interner = Interner::new();
        
        let name = interner.intern("test");
        let s = interner.get(name);
        
        assert_eq!(s.as_str(), "test");
    }

    #[test]
    fn test_name_size() {
        assert_eq!(std::mem::size_of::<Name>(), 4);
    }
}
