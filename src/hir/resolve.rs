//! Name resolution — resolving references to their definitions.
//!
//! This module provides name resolution for SysML/KerML.
//! It builds on top of the symbol extraction layer.
//!
//! # Architecture (January 2026)
//!
//! Name resolution follows a rust-analyzer inspired pattern:
//!
//! 1. **Symbol Extraction** - HIR extraction captures raw names/references with spans
//! 2. **Visibility Maps** - A separate pass builds per-scope visibility maps with resolved imports
//! 3. **Query-time Resolution** - Uses pre-computed visibility maps for O(1) lookups
//!
//! ## Key Data Structures
//!
//! - [`ScopeVisibility`] - Per-scope map of visible symbols (direct + imported)
//! - [`SymbolIndex`] - Global index with all symbols + pre-computed visibility maps
//! - [`Resolver`] - Query-time resolution using visibility maps

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::base::FileId;
use super::symbols::{HirSymbol, SymbolKind, TypeRef};

// ============================================================================
// SCOPE VISIBILITY (Pre-computed at index time)
// ============================================================================

/// Per-scope visibility map capturing what names are visible and where they resolve to.
///
/// Built once during index construction, used at query time for O(1) resolution.
///
/// # Example
///
/// For a scope like `ISQ` with `public import ISQSpaceTime::*`:
/// - `direct_defs` contains symbols defined directly in ISQ
/// - `imports` contains symbols from ISQSpaceTime (via the wildcard import)
/// - `public_reexports` tracks that ISQSpaceTime's symbols are re-exported
#[derive(Clone, Debug, Default)]
pub struct ScopeVisibility {
    /// The scope this visibility applies to (e.g., "ISQ", "Automotive::Torque").
    scope: Arc<str>,
    
    /// Symbols defined directly in this scope.
    /// SimpleName → QualifiedName
    direct_defs: HashMap<Arc<str>, Arc<str>>,
    
    /// Symbols visible via imports (includes transitive public re-exports).
    /// SimpleName → QualifiedName (the resolved target)
    imports: HashMap<Arc<str>, Arc<str>>,
    
    /// Namespaces that are publicly re-exported from this scope.
    /// Used for transitive import resolution.
    public_reexports: Vec<Arc<str>>,
}

impl ScopeVisibility {
    /// Create a new empty visibility map for a scope.
    pub fn new(scope: impl Into<Arc<str>>) -> Self {
        Self {
            scope: scope.into(),
            direct_defs: HashMap::new(),
            imports: HashMap::new(),
            public_reexports: Vec::new(),
        }
    }
    
    /// Get the scope this visibility applies to.
    pub fn scope(&self) -> &str {
        &self.scope
    }
    
    /// Look up a simple name in this scope's visibility.
    ///
    /// Checks direct definitions first, then imports.
    /// Returns the qualified name if found.
    pub fn lookup(&self, name: &str) -> Option<&Arc<str>> {
        self.direct_defs.get(name).or_else(|| self.imports.get(name))
    }
    
    /// Look up only in direct definitions.
    pub fn lookup_direct(&self, name: &str) -> Option<&Arc<str>> {
        self.direct_defs.get(name)
    }
    
    /// Look up only in imports.
    pub fn lookup_import(&self, name: &str) -> Option<&Arc<str>> {
        self.imports.get(name)
    }
    
    /// Add a direct definition to this scope.
    pub fn add_direct(&mut self, simple_name: Arc<str>, qualified_name: Arc<str>) {
        self.direct_defs.insert(simple_name, qualified_name);
    }
    
    /// Add an imported symbol to this scope.
    pub fn add_import(&mut self, simple_name: Arc<str>, qualified_name: Arc<str>) {
        // Don't overwrite direct definitions with imports
        if !self.direct_defs.contains_key(&simple_name) {
            self.imports.insert(simple_name, qualified_name);
        }
    }
    
    /// Add a public re-export (for transitive import resolution).
    pub fn add_public_reexport(&mut self, namespace: Arc<str>) {
        if !self.public_reexports.contains(&namespace) {
            self.public_reexports.push(namespace);
        }
    }
    
    /// Get all public re-exports.
    pub fn public_reexports(&self) -> &[Arc<str>] {
        &self.public_reexports
    }
    
    /// Get iterator over all direct definitions.
    pub fn direct_defs(&self) -> impl Iterator<Item = (&Arc<str>, &Arc<str>)> {
        self.direct_defs.iter()
    }
    
    /// Get iterator over all imports.
    pub fn imports(&self) -> impl Iterator<Item = (&Arc<str>, &Arc<str>)> {
        self.imports.iter()
    }
    
    /// Get count of visible symbols (direct + imported).
    pub fn len(&self) -> usize {
        self.direct_defs.len() + self.imports.len()
    }
    
    /// Check if visibility map is empty.
    pub fn is_empty(&self) -> bool {
        self.direct_defs.is_empty() && self.imports.is_empty()
    }
}

// ============================================================================
// SCOPE & RESOLUTION
// ============================================================================

/// A scope containing symbols that can be referenced.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    /// Symbols defined in this scope, keyed by simple name.
    symbols: HashMap<Arc<str>, Vec<HirSymbol>>,
    /// Parent scope for nested lookups.
    parent: Option<Arc<Scope>>,
    /// The qualified name prefix for this scope.
    prefix: Arc<str>,
}

impl Scope {
    /// Create a new empty root scope.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a child scope with a prefix.
    pub fn child(parent: Arc<Scope>, prefix: impl Into<Arc<str>>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent: Some(parent),
            prefix: prefix.into(),
        }
    }

    /// Add a symbol to this scope.
    pub fn add(&mut self, symbol: HirSymbol) {
        self.symbols
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol);
    }

    /// Look up a simple name in this scope and parent scopes.
    pub fn lookup(&self, name: &str) -> Option<&HirSymbol> {
        // First check this scope
        if let Some(symbols) = self.symbols.get(name) {
            // Return the first match (could be improved to handle overloading)
            return symbols.first();
        }
        // Then check parent
        if let Some(ref parent) = self.parent {
            return parent.lookup(name);
        }
        None
    }

    /// Look up all symbols with a given name (for overload resolution).
    pub fn lookup_all(&self, name: &str) -> Vec<&HirSymbol> {
        let mut results = Vec::new();
        
        if let Some(symbols) = self.symbols.get(name) {
            results.extend(symbols.iter());
        }
        
        if let Some(ref parent) = self.parent {
            results.extend(parent.lookup_all(name));
        }
        
        results
    }

    /// Look up a qualified name like "Package::Part::attr".
    pub fn lookup_qualified(&self, path: &str) -> Option<&HirSymbol> {
        let parts: Vec<&str> = path.split("::").collect();
        self.lookup_path(&parts)
    }

    /// Internal: look up by path segments.
    fn lookup_path(&self, parts: &[&str]) -> Option<&HirSymbol> {
        if parts.is_empty() {
            return None;
        }

        if parts.len() == 1 {
            return self.lookup(parts[0]);
        }

        // Find the first segment, then navigate into it
        let first = parts[0];
        if let Some(symbol) = self.lookup(first) {
            // For qualified lookup, we need to find nested symbols
            // This is a simplified version - real resolution would need
            // access to the child symbols of each scope
            let qualified = parts.join("::");
            return self.find_by_qualified_name(&qualified);
        }

        None
    }

    /// Find a symbol by its exact qualified name.
    fn find_by_qualified_name(&self, qualified: &str) -> Option<&HirSymbol> {
        for symbols in self.symbols.values() {
            for symbol in symbols {
                if symbol.qualified_name.as_ref() == qualified {
                    return Some(symbol);
                }
            }
        }
        if let Some(ref parent) = self.parent {
            return parent.find_by_qualified_name(qualified);
        }
        None
    }

    /// Get the qualified name prefix for this scope.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Get all symbols in this scope (not including parents).
    pub fn symbols(&self) -> impl Iterator<Item = &HirSymbol> {
        self.symbols.values().flatten()
    }

    /// Get the number of symbols in this scope.
    pub fn len(&self) -> usize {
        self.symbols.values().map(|v| v.len()).sum()
    }

    /// Check if the scope is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

// ============================================================================
// SYMBOL INDEX
// ============================================================================

/// Index into the symbols vector.
pub type SymbolIdx = usize;

/// An index of all symbols across multiple files.
///
/// This is the main data structure for workspace-wide name resolution.
/// It includes pre-computed visibility maps for efficient query-time resolution.
///
/// Symbols are stored in a single vector (`symbols`) and referenced by index
/// from all other maps. This ensures consistency when symbols are mutated
/// (e.g., when resolving type references).
#[derive(Clone, Debug, Default)]
pub struct SymbolIndex {
    /// The single source of truth for all symbols.
    symbols: Vec<HirSymbol>,
    /// Index by qualified name -> symbol index.
    by_qualified_name: HashMap<Arc<str>, SymbolIdx>,
    /// Index by simple name -> symbol indices (may have multiple).
    by_simple_name: HashMap<Arc<str>, Vec<SymbolIdx>>,
    /// Index by file -> symbol indices.
    by_file: HashMap<FileId, Vec<SymbolIdx>>,
    /// Definitions only (not usages) -> symbol indices.
    definitions: HashMap<Arc<str>, SymbolIdx>,
    /// Pre-computed visibility map for each scope (built after all files added).
    visibility_map: HashMap<Arc<str>, ScopeVisibility>,
    /// Flag to track if visibility maps are stale and need rebuilding.
    visibility_dirty: bool,
}

impl SymbolIndex {
    /// Create a new empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add symbols from a file to the index.
    pub fn add_file(&mut self, file: FileId, symbols: Vec<HirSymbol>) {
        // Remove existing symbols from this file first
        self.remove_file(file);
        
        // Mark visibility maps as dirty
        self.visibility_dirty = true;

        let mut file_indices = Vec::with_capacity(symbols.len());
        
        for symbol in symbols {
            let idx = self.symbols.len();
            
            // Index by qualified name
            self.by_qualified_name
                .insert(symbol.qualified_name.clone(), idx);

            // Index by simple name
            self.by_simple_name
                .entry(symbol.name.clone())
                .or_default()
                .push(idx);

            // Track definitions separately
            if symbol.kind.is_definition() {
                self.definitions
                    .insert(symbol.qualified_name.clone(), idx);
            }

            // Track for file index
            file_indices.push(idx);
            
            // Store the symbol
            self.symbols.push(symbol);
        }
        
        // Index by file
        self.by_file.insert(file, file_indices);
    }

    /// Remove all symbols from a file.
    /// 
    /// Note: This marks indices as invalid but doesn't compact the symbols vec
    /// to avoid invalidating other indices. For a full cleanup, rebuild the index.
    pub fn remove_file(&mut self, file: FileId) {
        if let Some(indices) = self.by_file.remove(&file) {
            // Mark visibility maps as dirty
            self.visibility_dirty = true;
            
            for &idx in &indices {
                if let Some(symbol) = self.symbols.get(idx) {
                    let qname = symbol.qualified_name.clone();
                    let sname = symbol.name.clone();
                    
                    self.by_qualified_name.remove(&qname);
                    self.definitions.remove(&qname);

                    // Remove from simple name index
                    if let Some(list) = self.by_simple_name.get_mut(&sname) {
                        list.retain(|&i| i != idx);
                        if list.is_empty() {
                            self.by_simple_name.remove(&sname);
                        }
                    }
                }
            }
            // Note: We don't remove from self.symbols to preserve indices
            // A rebuild would be needed for true cleanup
        }
    }

    /// Look up a symbol by qualified name.
    pub fn lookup_qualified(&self, name: &str) -> Option<&HirSymbol> {
        self.by_qualified_name
            .get(name)
            .and_then(|&idx| self.symbols.get(idx))
    }
    
    /// Look up a symbol by qualified name (mutable).
    pub fn lookup_qualified_mut(&mut self, name: &str) -> Option<&mut HirSymbol> {
        self.by_qualified_name
            .get(name)
            .copied()
            .and_then(move |idx| self.symbols.get_mut(idx))
    }

    /// Look up all symbols with a simple name.
    pub fn lookup_simple(&self, name: &str) -> Vec<&HirSymbol> {
        self.by_simple_name
            .get(name)
            .map(|indices| {
                indices.iter()
                    .filter_map(|&idx| self.symbols.get(idx))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Look up a definition by qualified name.
    pub fn lookup_definition(&self, name: &str) -> Option<&HirSymbol> {
        self.definitions
            .get(name)
            .and_then(|&idx| self.symbols.get(idx))
    }

    /// Get all symbols in a file.
    pub fn symbols_in_file(&self, file: FileId) -> Vec<&HirSymbol> {
        self.by_file
            .get(&file)
            .map(|indices| {
                indices.iter()
                    .filter_map(|&idx| self.symbols.get(idx))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all definitions in the index.
    pub fn all_definitions(&self) -> impl Iterator<Item = &HirSymbol> {
        self.definitions
            .values()
            .filter_map(|&idx| self.symbols.get(idx))
    }

    /// Get all symbols in the index.
    pub fn all_symbols(&self) -> impl Iterator<Item = &HirSymbol> {
        self.by_qualified_name
            .values()
            .filter_map(|&idx| self.symbols.get(idx))
    }

    /// Get the total number of symbols.
    pub fn len(&self) -> usize {
        self.by_qualified_name.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.by_qualified_name.is_empty()
    }

    /// Get number of files indexed.
    pub fn file_count(&self) -> usize {
        self.by_file.len()
    }
    
    // ========================================================================
    // VISIBILITY MAP CONSTRUCTION
    // ========================================================================
    
    /// Ensure visibility maps are up-to-date, rebuilding if necessary.
    ///
    /// Call this before using visibility-based resolution.
    pub fn ensure_visibility_maps(&mut self) {
        if self.visibility_dirty {
            self.build_visibility_maps();
            self.visibility_dirty = false;
        }
    }
    
    /// Resolve all type references in all symbols.
    ///
    /// This is called after visibility maps are built to fill in `resolved_target`
    /// on all TypeRefs. This is the "semantic resolution pass" that pre-computes
    /// what each type reference points to.
    /// 
    /// Feature chains (like `takePicture.focus`) are now preserved explicitly
    /// as TypeRefKind::Chain from the parser. Simple refs use TypeRefKind::Simple.
    pub fn resolve_all_type_refs(&mut self) {
        use crate::hir::symbols::{TypeRefKind, TypeRefChain};
        
        // Ensure visibility maps are built first
        self.ensure_visibility_maps();
        
        // Collect work items
        // For Simple: (sym_idx, tr_idx, target, chain_context)
        // For Chain: we'll resolve each part with explicit chain context
        let mut work: Vec<(SymbolIdx, usize, usize, Arc<str>, Option<(Vec<Arc<str>>, usize)>)> = Vec::new();
        
        for (sym_idx, sym) in self.symbols.iter().enumerate() {
            for (trk_idx, trk) in sym.type_refs.iter().enumerate() {
                match trk {
                    TypeRefKind::Simple(tr) => {
                        // Simple refs might still be part of chains detected from spans (legacy)
                        // For now, treat them as standalone
                        work.push((sym_idx, trk_idx, 0, tr.target.clone(), None));
                    }
                    TypeRefKind::Chain(chain) => {
                        // Chain parts have explicit chain context
                        let chain_parts: Vec<Arc<str>> = chain.parts.iter()
                            .map(|p| p.target.clone())
                            .collect();
                        for (part_idx, part) in chain.parts.iter().enumerate() {
                            work.push((sym_idx, trk_idx, part_idx, part.target.clone(), 
                                Some((chain_parts.clone(), part_idx))));
                        }
                    }
                }
            }
        }
        
        // Now resolve each type_ref
        for (sym_idx, trk_idx, part_idx, target, chain_context) in work {
            // Get symbol info for resolution (need scope)
            let symbol_qname = self.symbols[sym_idx].qualified_name.clone();
            let resolved = self.resolve_type_ref(&symbol_qname, &target, &chain_context);
            
            // Update the type_ref directly
            if let Some(trk) = self.symbols[sym_idx].type_refs.get_mut(trk_idx) {
                match trk {
                    TypeRefKind::Simple(tr) => {
                        tr.resolved_target = resolved;
                    }
                    TypeRefKind::Chain(chain) => {
                        if let Some(part) = chain.parts.get_mut(part_idx) {
                            part.resolved_target = resolved;
                        }
                    }
                }
            }
        }
    }
    
    /// Resolve a single type reference within a symbol's scope.
    ///
    /// For regular references: uses lexical scoping + imports
    /// For feature chain members: resolves through type membership
    fn resolve_type_ref(
        &self,
        containing_symbol: &str,
        target: &str,
        chain_context: &Option<(Vec<Arc<str>>, usize)>,
    ) -> Option<Arc<str>> {
        // Get the scope for resolution
        // For expressions inside a symbol, we should look in the containing symbol's scope
        // (not just its parent) because siblings might be in scope
        let scope = containing_symbol;
        
        // Check if this is a feature chain member (index > 0)
        if let Some((chain_parts, chain_idx)) = chain_context {
            if *chain_idx > 0 {
                // This is a member access like `obj.field`
                // We need to resolve `obj` first, get its type, then resolve `field` within that type
                return self.resolve_feature_chain_member(scope, chain_parts, *chain_idx);
            }
        }
        
        // Regular lexical resolution - use scope walk to search hierarchy
        if let Some(sym) = self.resolve_with_scope_walk(target, scope) {
            return Some(sym.qualified_name.clone());
        }
        
        // Try qualified name directly
        self.lookup_qualified(target).map(|s| s.qualified_name.clone())
    }
    
    /// Follow a typing chain to find the actual type definition.
    /// 
    /// For example, if we have:
    ///   action takePicture : TakePicture;  // usage typed by definition
    ///   action a :> takePicture;           // usage subsets usage
    /// 
    /// When resolving from `a`, we need to follow: a -> takePicture -> TakePicture
    /// 
    /// IMPORTANT: If the input symbol is already a definition, return it immediately.
    /// We only follow the chain for usages, not for definition inheritance.
    fn follow_typing_chain(&self, sym: &HirSymbol, scope: &str) -> Arc<str> {
        // If the input is already a definition, return it - don't follow inheritance
        if sym.kind.is_definition() {
            return sym.qualified_name.clone();
        }
        
        let mut current_qname = sym.qualified_name.clone();
        let mut visited = std::collections::HashSet::new();
        visited.insert(current_qname.clone());
        
        // Keep following supertypes until we find a definition or loop
        loop {
            let current = match self.lookup_qualified(&current_qname) {
                Some(s) => s,
                None => break,
            };
            
            let Some(type_name) = current.supertypes.first() else {
                // No supertypes
                break;
            };
            
            let type_resolver = Resolver::new(self).with_scope(scope);
            match type_resolver.resolve(type_name) {
                ResolveResult::Found(type_sym) => {
                    if visited.contains(&type_sym.qualified_name) {
                        // Cycle detected, stop here
                        break;
                    }
                    visited.insert(type_sym.qualified_name.clone());
                    
                    // If this symbol is a definition, stop
                    if type_sym.kind.is_definition() {
                        return type_sym.qualified_name.clone();
                    }
                    
                    // Otherwise continue following
                    current_qname = type_sym.qualified_name.clone();
                }
                _ => {
                    // Can't resolve further, use what we have
                    break;
                }
            }
        }
        
        current_qname
    }
    
    /// Resolve a feature chain member (e.g., `focus` in `takePicture.focus`).
    ///
    /// Chain resolution follows rust-analyzer's approach:
    /// 1. Resolve first part using full lexical scoping (walks up parent scopes)
    /// 2. Get that symbol's type definition
    /// 3. Resolve subsequent parts as members of that type
    /// 4. For each member, follow its type to resolve the next part
    /// 
    /// IMPORTANT: SysML usages can have nested members defined directly within them,
    /// even when they have a type annotation. We must check the usage's own scope
    /// BEFORE falling back to its type definition.
    fn resolve_feature_chain_member(
        &self,
        scope: &str,
        chain_parts: &[Arc<str>],
        chain_idx: usize,
    ) -> Option<Arc<str>> {
        if chain_idx == 0 || chain_parts.is_empty() {
            return None;
        }
        
        // Step 1: Resolve the first part using full lexical scoping
        // This walks up the scope hierarchy to find the symbol
        let first_part = &chain_parts[0];
        let first_sym = self.resolve_with_scope_walk(first_part, scope)?;
        
        // Track the current symbol (for checking nested members) and its type scope (for inheritance)
        let mut current_sym_qname = first_sym.qualified_name.clone();
        let mut current_type_scope = self.get_member_lookup_scope(&first_sym, scope);
        
        // Step 2: Walk through the chain, resolving each part
        for i in 1..=chain_idx {
            let part = &chain_parts[i];
            
            // SysML Pattern: Usages can have nested members defined directly within them.
            // For example: part differential:Differential { port leftDiffPort:DiffPort; }
            // Here `leftDiffPort` is a member of the usage, not the Differential definition.
            //
            // Strategy: First try to find member in the symbol's own scope (nested members),
            // then fall back to the type scope (inherited members).
            
            let member_sym = {
                // Try 1: Look for nested member directly in the current symbol
                if let Some(sym) = self.find_member_in_scope(&current_sym_qname, part) {
                    sym
                } else if current_sym_qname != current_type_scope {
                    // Try 2: Look in the type scope (inherited members)
                    self.find_member_in_scope(&current_type_scope, part)?
                } else {
                    return None;
                }
            };
            
            
            if i == chain_idx {
                // This is the target - return it
                return Some(member_sym.qualified_name.clone());
            }
            
            // Update for next iteration: track both the symbol and its type scope
            current_sym_qname = member_sym.qualified_name.clone();
            current_type_scope = self.get_member_lookup_scope(&member_sym, scope);
        }
        
        None
    }
    
    /// Resolve a name by walking up the scope hierarchy.
    /// This is the core lexical scoping resolution.
    fn resolve_with_scope_walk(&self, name: &str, starting_scope: &str) -> Option<HirSymbol> {
        let mut current_scope: Arc<str> = Arc::from(starting_scope);
        
        loop {
            // Try to resolve in current scope (visibility maps include inherited members)
            let resolver = Resolver::new(self).with_scope(current_scope.clone());
            if let ResolveResult::Found(sym) = resolver.resolve(name) {
                return Some(sym);
            }
            
            // Walk up to parent scope
            if current_scope.is_empty() {
                break;
            }
            current_scope = Arc::from(Self::parent_scope(&current_scope).unwrap_or(""));
        }
        
        // Final attempt: try global lookup
        self.lookup_qualified(name).cloned()
    }
    
    /// Get the scope to use for member lookups on a symbol.
    /// If the symbol has a type, returns the type's qualified name.
    /// Otherwise, returns the symbol's own qualified name (for nested members).
    /// 
    /// IMPORTANT: For usages typed by other usages, we return the typing usage
    /// (not its definition) because usages can have their own nested members.
    fn get_member_lookup_scope(&self, sym: &HirSymbol, resolution_scope: &str) -> Arc<str> {
        // If symbol has a type annotation, resolve and use that type
        if let Some(type_name) = sym.supertypes.first() {
            // Try to resolve the type name
            let sym_scope = Self::parent_scope(&sym.qualified_name).unwrap_or("");
            
            if let Some(type_sym) = self.resolve_with_scope_walk(type_name, sym_scope) {
                // If the resolved type is a USAGE, return it directly (it may have nested members)
                // Only follow typing chain for DEFINITIONS
                if type_sym.kind.is_usage() {
                    return type_sym.qualified_name.clone();
                }
                // Follow the typing chain to get the actual definition
                let final_type = self.follow_typing_chain(&type_sym, resolution_scope);
                return final_type;
            }
            
            // Type name might already be qualified
            if let Some(type_sym) = self.lookup_qualified(type_name) {
                if type_sym.kind.is_usage() {
                    return type_sym.qualified_name.clone();
                }
                let final_type = self.follow_typing_chain(type_sym, resolution_scope);
                return final_type;
            }
            
        }
        
        // No type - use the symbol itself as the scope for nested members
        sym.qualified_name.clone()
    }
    
    /// Find a member within a type scope.
    /// Tries direct lookup, then searches inherited members from supertypes.
    pub fn find_member_in_scope(&self, type_scope: &str, member_name: &str) -> Option<HirSymbol> {
        
        // Strategy 1: Direct qualified lookup
        let direct_qname = format!("{}::{}", type_scope, member_name);
        if let Some(sym) = self.lookup_qualified(&direct_qname) {
            return Some(sym.clone());
        }
        
        // Strategy 2: Check visibility map for the type scope
        if let Some(vis) = self.visibility_for_scope(type_scope) {
            if let Some(qname) = vis.lookup(member_name) {
                if let Some(sym) = self.lookup_qualified(qname) {
                    return Some(sym.clone());
                }
            } else {
            }
        } else {
        }
        
        // Strategy 3: Look in supertypes (inheritance)
        if let Some(type_sym) = self.lookup_qualified(type_scope) {
            for supertype in &type_sym.supertypes {
                // Resolve the supertype name
                let parent_scope = Self::parent_scope(type_scope).unwrap_or("");
                if let Some(super_sym) = self.resolve_with_scope_walk(supertype, parent_scope) {
                    // Recursively search in the supertype
                    if let Some(found) = self.find_member_in_scope(&super_sym.qualified_name, member_name) {
                        return Some(found);
                    }
                } else {
                }
            }
        } else {
        }
        
        None
    }
    
    /// Get the visibility map for a scope (if built).
    pub fn visibility_for_scope(&self, scope: &str) -> Option<&ScopeVisibility> {
        self.visibility_map.get(scope)
    }
    
    /// Build visibility maps for all scopes.
    ///
    /// This is the main entry point for constructing visibility information.
    /// It performs:
    /// 1. Scope collection (packages, definitions with bodies)
    /// 2. Direct definition collection
    /// 3. Inheritance propagation (supertypes' members become visible)
    /// 4. Import processing with transitive public re-export handling
    fn build_visibility_maps(&mut self) {
        // 1. Collect all scopes (packages, namespaces, definitions that contain members)
        let scopes = self.collect_all_scopes();
        
        // 2. Initialize visibility maps with direct definitions
        self.visibility_map.clear();
        for scope in &scopes {
            let mut vis = ScopeVisibility::new(scope.clone());
            self.collect_direct_defs(&mut vis, scope);
            self.visibility_map.insert(scope.clone(), vis);
        }
        
        // Also create a root scope (empty string) for global visibility
        let mut root_vis = ScopeVisibility::new("");
        self.collect_direct_defs(&mut root_vis, "");
        self.visibility_map.insert(Arc::from(""), root_vis);
        
        // 3. Propagate inherited members from supertypes
        self.propagate_inherited_members();
        
        // 4. Process all imports (track visited to handle transitive re-exports)
        let mut visited: HashSet<(Arc<str>, Arc<str>)> = HashSet::new();
        let scope_keys: Vec<_> = self.visibility_map.keys().cloned().collect();
        
        for scope in scope_keys {
            self.process_imports_recursive(&scope, &mut visited);
        }
    }
    
    /// Propagate inherited members from supertypes into scope visibility maps.
    /// When `Shape :> Path`, members of `Path` become visible in `Shape`.
    fn propagate_inherited_members(&mut self) {
        // Collect inheritance info: (scope, resolved_supertype_qname)
        let mut inheritance_pairs: Vec<(Arc<str>, Arc<str>)> = Vec::new();
        
        for symbol in &self.symbols {
            if !symbol.supertypes.is_empty() {
                let scope = &symbol.qualified_name;
                let parent_scope = Self::parent_scope(scope).unwrap_or("");
                
                for supertype in &symbol.supertypes {
                    // Resolve supertype name to qualified name
                    if let Some(resolved) = self.resolve_supertype_for_inheritance(supertype, parent_scope) {
                        inheritance_pairs.push((scope.clone(), resolved));
                    }
                }
            }
        }
        
        // Now propagate: for each (child_scope, parent_scope), add parent's direct members to child
        for (child_scope, parent_scope) in inheritance_pairs {
            // Get parent's direct members
            let parent_members: Vec<(Arc<str>, Arc<str>)> = self.visibility_map
                .get(&parent_scope)
                .map(|vis| vis.direct_defs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            
            // Add to child's visibility (if not already present - direct takes priority)
            if let Some(child_vis) = self.visibility_map.get_mut(&child_scope) {
                for (name, qname) in parent_members {
                    if !child_vis.direct_defs.contains_key(&name) {
                        child_vis.direct_defs.insert(name, qname);
                    }
                }
            }
        }
    }
    
    /// Resolve a supertype reference for inheritance propagation.
    /// Uses simple lookup without full resolution to avoid infinite loops.
    fn resolve_supertype_for_inheritance(&self, name: &str, starting_scope: &str) -> Option<Arc<str>> {
        // Try qualified lookup first
        if let Some(sym) = self.lookup_qualified(name) {
            return Some(sym.qualified_name.clone());
        }
        
        // Walk up scopes looking for the name
        let mut current_scope = starting_scope;
        loop {
            // Try direct qualified name in this scope
            let qname = if current_scope.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", current_scope, name)
            };
            
            if let Some(sym) = self.lookup_qualified(&qname) {
                return Some(sym.qualified_name.clone());
            }
            
            // Check visibility map for this scope
            if let Some(vis) = self.visibility_map.get(current_scope) {
                if let Some(resolved) = vis.direct_defs.get(name) {
                    return Some(resolved.clone());
                }
            }
            
            if current_scope.is_empty() {
                break;
            }
            current_scope = Self::parent_scope(current_scope).unwrap_or("");
        }
        
        None
    }
    
    /// Process imports for a scope recursively, handling transitive public re-exports.
    fn process_imports_recursive(&mut self, scope: &str, visited: &mut HashSet<(Arc<str>, Arc<str>)>) {
        // Find import symbols in this scope
        let imports_to_process: Vec<_> = self.symbols.iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .filter(|s| {
                let qname = s.qualified_name.as_ref();
                if let Some(idx) = qname.find("::import:") {
                    &qname[..idx] == scope
                } else if qname.starts_with("import:") {
                    scope.is_empty()
                } else {
                    false
                }
            })
            .cloned()
            .collect();
        
        for import_symbol in imports_to_process {
            let is_wildcard = import_symbol.name.ends_with("::*");
            let import_target = import_symbol.name.trim_end_matches("::*");
            let resolved_target = self.resolve_import_target(scope, import_target);
            
            if is_wildcard {
                // Wildcard import: import all symbols from target scope
                
                // Skip if already visited this (scope, target) pair
                let key = (Arc::from(scope), Arc::from(resolved_target.as_str()));
                if visited.contains(&key) {
                    continue;
                }
                visited.insert(key);
                
                // Recursively process the target's imports first (to get transitive symbols)
                self.process_imports_recursive(&resolved_target, visited);
                
                // Now copy symbols from target to this scope
                if let Some(target_vis) = self.visibility_map.get(&resolved_target as &str).cloned() {
                    let vis = self.visibility_map.get_mut(scope).expect("scope must exist");
                    
                    for (name, qname) in target_vis.direct_defs() {
                        vis.add_import(name.clone(), qname.clone());
                    }
                    for (name, qname) in target_vis.imports() {
                        vis.add_import(name.clone(), qname.clone());
                    }
                    
                    if import_symbol.is_public {
                        vis.add_public_reexport(Arc::from(resolved_target.as_str()));
                    }
                }
            } else {
                // Specific import: import a single symbol
                // E.g., `import EngineDefs::Engine;` makes `Engine` visible as `EngineDefs::Engine`
                
                // Get the simple name (last component of path)
                let simple_name = resolved_target.rsplit("::").next().unwrap_or(&resolved_target);
                
                // Add to this scope's imports
                if let Some(vis) = self.visibility_map.get_mut(scope) {
                    vis.add_import(Arc::from(simple_name), Arc::from(resolved_target.as_str()));
                }
            }
        }
    }
    
    /// Collect all scopes that should have visibility maps.
    ///
    /// A scope is any namespace that can contain definitions:
    /// - Packages
    /// - Definition types (PartDef, ActionDef, etc.) that have nested members
    fn collect_all_scopes(&self) -> Vec<Arc<str>> {
        let mut scopes = HashSet::new();
        
        for symbol in &self.symbols {
            // The symbol's parent scope should be tracked
            if let Some(parent) = Self::parent_scope(&symbol.qualified_name) {
                scopes.insert(Arc::from(parent));
            }
            
            // If this is a namespace-creating symbol (Package, *Def), it's a scope
            if symbol.kind == SymbolKind::Package || symbol.kind.is_definition() {
                scopes.insert(symbol.qualified_name.clone());
            }
        }
        
        scopes.into_iter().collect()
    }
    
    /// Collect direct definitions for a scope.
    ///
    /// These are symbols whose immediate parent is this scope.
    fn collect_direct_defs(&self, vis: &mut ScopeVisibility, scope: &str) {
        for symbol in &self.symbols {
            // Check if this symbol is a direct child of the scope
            if let Some(parent) = Self::parent_scope(&symbol.qualified_name) {
                if parent == scope {
                    // Debug: log if this is a Requirements symbol
                    if symbol.name.as_ref() == "Requirements" {
                    }
                    vis.add_direct(symbol.name.clone(), symbol.qualified_name.clone());
                    
                    // Also register by short_name if available
                    if let Some(ref short_name) = symbol.short_name {
                        vis.add_direct(short_name.clone(), symbol.qualified_name.clone());
                    }
                }
            } else if scope.is_empty() {
                // Root-level symbols belong to the empty scope
                if symbol.name.as_ref() == "Requirements" {
                }
                vis.add_direct(symbol.name.clone(), symbol.qualified_name.clone());
                
                // Also register by short_name if available
                if let Some(ref short_name) = symbol.short_name {
                    vis.add_direct(short_name.clone(), symbol.qualified_name.clone());
                }
            }
        }
    }
    
    /// Resolve an import target relative to a scope.
    ///
    /// Checks: current scope, parent scopes, then global.
    fn resolve_import_target(&self, scope: &str, target: &str) -> String {
        // If already qualified with ::, check as-is first
        if target.contains("::") {
            if self.visibility_map.contains_key(target) {
                return target.to_string();
            }
        }
        
        // Try relative to scope and parent scopes
        let mut current = scope.to_string();
        loop {
            let candidate = if current.is_empty() {
                target.to_string()
            } else {
                format!("{}::{}", current, target)
            };
            
            if self.visibility_map.contains_key(&candidate as &str) {
                return candidate;
            }
            
            // Move up
            if let Some(idx) = current.rfind("::") {
                current = current[..idx].to_string();
            } else if !current.is_empty() {
                current = String::new();
            } else {
                break;
            }
        }
        
        // Fall back to target as-is (might be global)
        target.to_string()
    }
    
    /// Get the parent scope of a qualified name.
    ///
    /// "A::B::C" -> Some("A::B")
    /// "A" -> Some("")
    /// "" -> None
    fn parent_scope(qualified_name: &str) -> Option<&str> {
        if qualified_name.is_empty() {
            return None;
        }
        match qualified_name.rfind("::") {
            Some(idx) => Some(&qualified_name[..idx]),
            None => Some(""), // Root level
        }
    }

    /// Build a resolver for the given scope.
    ///
    /// The resolver uses pre-computed visibility maps for efficient resolution.
    /// No need to manually collect imports - they're already in the visibility map.
    pub fn resolver_for_scope(&self, scope: &str) -> Resolver<'_> {
        Resolver::new(self).with_scope(scope)
    }
}

// ============================================================================
// SYMBOL KIND HELPERS
// ============================================================================

impl SymbolKind {
    /// Check if this is a definition kind (vs usage).
    pub fn is_definition(&self) -> bool {
        matches!(
            self,
            SymbolKind::Package
                | SymbolKind::PartDef
                | SymbolKind::ItemDef
                | SymbolKind::ActionDef
                | SymbolKind::PortDef
                | SymbolKind::AttributeDef
                | SymbolKind::ConnectionDef
                | SymbolKind::InterfaceDef
                | SymbolKind::AllocationDef
                | SymbolKind::RequirementDef
                | SymbolKind::ConstraintDef
                | SymbolKind::StateDef
                | SymbolKind::CalculationDef
                | SymbolKind::UseCaseDef
                | SymbolKind::AnalysisCaseDef
                | SymbolKind::ConcernDef
                | SymbolKind::ViewDef
                | SymbolKind::ViewpointDef
                | SymbolKind::RenderingDef
                | SymbolKind::EnumerationDef
        )
    }

    /// Check if this is a usage kind.
    pub fn is_usage(&self) -> bool {
        matches!(
            self,
            SymbolKind::PartUsage
                | SymbolKind::ItemUsage
                | SymbolKind::ActionUsage
                | SymbolKind::PortUsage
                | SymbolKind::AttributeUsage
                | SymbolKind::ConnectionUsage
                | SymbolKind::InterfaceUsage
                | SymbolKind::AllocationUsage
                | SymbolKind::RequirementUsage
                | SymbolKind::ConstraintUsage
                | SymbolKind::StateUsage
                | SymbolKind::CalculationUsage
                | SymbolKind::ReferenceUsage
                | SymbolKind::OccurrenceUsage
                | SymbolKind::FlowUsage
        )
    }

    /// Get the corresponding definition kind for a usage.
    pub fn to_definition_kind(&self) -> Option<SymbolKind> {
        match self {
            SymbolKind::PartUsage => Some(SymbolKind::PartDef),
            SymbolKind::ItemUsage => Some(SymbolKind::ItemDef),
            SymbolKind::ActionUsage => Some(SymbolKind::ActionDef),
            SymbolKind::PortUsage => Some(SymbolKind::PortDef),
            SymbolKind::AttributeUsage => Some(SymbolKind::AttributeDef),
            SymbolKind::ConnectionUsage => Some(SymbolKind::ConnectionDef),
            SymbolKind::InterfaceUsage => Some(SymbolKind::InterfaceDef),
            SymbolKind::AllocationUsage => Some(SymbolKind::AllocationDef),
            SymbolKind::RequirementUsage => Some(SymbolKind::RequirementDef),
            SymbolKind::ConstraintUsage => Some(SymbolKind::ConstraintDef),
            SymbolKind::StateUsage => Some(SymbolKind::StateDef),
            SymbolKind::CalculationUsage => Some(SymbolKind::CalculationDef),
            _ => None,
        }
    }
}

// ============================================================================
// RESOLUTION RESULT
// ============================================================================

/// Result of resolving a reference.
#[derive(Clone, Debug)]
pub enum ResolveResult {
    /// Successfully resolved to a single symbol.
    Found(HirSymbol),
    /// Resolved to multiple candidates (ambiguous).
    Ambiguous(Vec<HirSymbol>),
    /// Could not resolve the reference.
    NotFound,
}

impl ResolveResult {
    /// Get the resolved symbol if unambiguous.
    pub fn symbol(&self) -> Option<&HirSymbol> {
        match self {
            ResolveResult::Found(s) => Some(s),
            _ => None,
        }
    }

    /// Check if resolution was successful.
    pub fn is_found(&self) -> bool {
        matches!(self, ResolveResult::Found(_))
    }

    /// Check if the reference was ambiguous.
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, ResolveResult::Ambiguous(_))
    }
}

// ============================================================================
// RESOLVER
// ============================================================================

/// Resolver for name lookups using pre-computed visibility maps.
///
/// The resolver uses visibility maps built during index construction,
/// so there's no need to manually configure imports.
#[derive(Clone, Debug)]
pub struct Resolver<'a> {
    /// The symbol index to search.
    index: &'a SymbolIndex,
    /// Current scope prefix (e.g., "Vehicle::Powertrain").
    current_scope: Arc<str>,
}

impl<'a> Resolver<'a> {
    /// Create a new resolver.
    pub fn new(index: &'a SymbolIndex) -> Self {
        Self {
            index,
            current_scope: Arc::from(""),
        }
    }

    /// Set the current scope.
    pub fn with_scope(mut self, scope: impl Into<Arc<str>>) -> Self {
        self.current_scope = scope.into();
        self
    }
    
    /// Resolve a name using pre-computed visibility maps.
    pub fn resolve(&self, name: &str) -> ResolveResult {
        
        // 1. Handle qualified paths like "ISQ::TorqueValue" 
        if name.contains("::") {
            // For qualified paths, try exact match first
            if let Some(symbol) = self.index.lookup_qualified(name) {
                return ResolveResult::Found(symbol.clone());
            }
            return self.resolve_qualified_path(name);
        }
        
        // 2. For simple names, try scope walking FIRST (finds local Requirements before global)
        let mut current = self.current_scope.to_string();
        loop {
            if let Some(vis) = self.index.visibility_for_scope(&current) {
                // Check direct definitions first (higher priority)
                if let Some(qname) = vis.lookup_direct(name) {
                    if let Some(sym) = self.index.lookup_qualified(qname) {
                        return ResolveResult::Found(sym.clone());
                    }
                }
                
                // Check imports
                if let Some(qname) = vis.lookup_import(name) {
                    if let Some(sym) = self.index.lookup_qualified(qname) {
                        return ResolveResult::Found(sym.clone());
                    }
                }
            }
            
            // Move up to parent scope
            if let Some(idx) = current.rfind("::") {
                current = current[..idx].to_string();
            } else if !current.is_empty() {
                current = String::new(); // Try root scope
            } else {
                break;
            }
        }
        
        // 3. Fall back to exact qualified match for simple names
        // This handles cases like a global package named exactly "Requirements"
        if let Some(symbol) = self.index.lookup_qualified(name) {
            return ResolveResult::Found(symbol.clone());
        }
        
        ResolveResult::NotFound
    }
    
    /// Resolve a qualified path like "ISQ::TorqueValue" using visibility maps.
    ///
    /// This handles cases where:
    /// - ISQ is a package with `public import ISQSpaceTime::*`
    /// - TorqueValue is defined in ISQSpaceTime
    fn resolve_qualified_path(&self, path: &str) -> ResolveResult {
        let (first, rest) = match path.find("::") {
            Some(idx) => (&path[..idx], &path[idx + 2..]),
            None => return ResolveResult::NotFound,
        };
        
        
        // Resolve the first segment (it's a simple name, so resolve() won't recurse here)
        let first_sym = self.resolve(first);
        
        match first_sym {
            ResolveResult::Found(first_symbol) => {
                // Get the target scope (follow alias if needed)
                let target_scope = if first_symbol.kind == SymbolKind::Alias {
                    if let Some(target) = first_symbol.supertypes.first() {
                        target.as_ref()
                    } else {
                        first_symbol.qualified_name.as_ref()
                    }
                } else {
                    first_symbol.qualified_name.as_ref()
                };
                
                // Handle nested qualified paths (e.g., "A::B::C" where rest="B::C")
                if rest.contains("::") {
                    // Recursively resolve with target scope
                    let nested_resolver = Resolver::new(self.index).with_scope(target_scope);
                    return nested_resolver.resolve(rest);
                }
                
                // Look up 'rest' in target scope's visibility map
                if let Some(vis) = self.index.visibility_for_scope(target_scope) {
                    // Check direct definitions first
                    if let Some(qname) = vis.lookup_direct(rest) {
                        if let Some(sym) = self.index.lookup_qualified(qname) {
                            return ResolveResult::Found(sym.clone());
                        }
                    }
                    
                    // Check imports (handles public import ISQSpaceTime::*)
                    if let Some(qname) = vis.lookup_import(rest) {
                        if let Some(sym) = self.index.lookup_qualified(qname) {
                            return ResolveResult::Found(sym.clone());
                        }
                    }
                }
                
                // Try direct qualified lookup (might be nested definition)
                let full_path = format!("{}::{}", target_scope, rest);
                if let Some(sym) = self.index.lookup_qualified(&full_path) {
                    return ResolveResult::Found(sym.clone());
                }
            }
            _ => {}
        }
        
        ResolveResult::NotFound
    }

    /// Resolve a type reference (for : Type annotations).
    pub fn resolve_type(&self, name: &str) -> ResolveResult {
        let result = self.resolve(name);
        
        // Filter to only definition kinds
        match result {
            ResolveResult::Found(ref symbol) if symbol.kind.is_definition() => result,
            ResolveResult::Found(_) => ResolveResult::NotFound,
            ResolveResult::Ambiguous(symbols) => {
                let defs: Vec<_> = symbols.into_iter().filter(|s| s.kind.is_definition()).collect();
                match defs.len() {
                    0 => ResolveResult::NotFound,
                    1 => ResolveResult::Found(defs.into_iter().next().unwrap()),
                    _ => ResolveResult::Ambiguous(defs),
                }
            }
            ResolveResult::NotFound => ResolveResult::NotFound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_symbol(name: &str, qualified: &str, kind: SymbolKind, file: u32) -> HirSymbol {
        HirSymbol {
            name: Arc::from(name),
            short_name: None,
            qualified_name: Arc::from(qualified),
            kind,
            file: FileId::new(file),
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            doc: None,
            supertypes: Vec::new(),
            type_refs: Vec::new(),
            is_public: false,
        }
    }

    #[test]
    fn test_scope_lookup() {
        let mut scope = Scope::new();
        scope.add(make_symbol("Car", "Car", SymbolKind::PartDef, 0));
        scope.add(make_symbol("Engine", "Engine", SymbolKind::PartDef, 0));

        assert!(scope.lookup("Car").is_some());
        assert!(scope.lookup("Engine").is_some());
        assert!(scope.lookup("Unknown").is_none());
    }

    #[test]
    fn test_scope_child() {
        let mut root = Scope::new();
        root.add(make_symbol("Global", "Global", SymbolKind::Package, 0));
        
        let root = Arc::new(root);
        let mut child = Scope::child(root, "Child");
        child.add(make_symbol("Local", "Child::Local", SymbolKind::PartDef, 0));

        // Child can see its own symbols
        assert!(child.lookup("Local").is_some());
        // Child can see parent symbols
        assert!(child.lookup("Global").is_some());
    }

    #[test]
    fn test_symbol_index_basic() {
        let mut index = SymbolIndex::new();
        
        let symbols = vec![
            make_symbol("Vehicle", "Vehicle", SymbolKind::Package, 0),
            make_symbol("Car", "Vehicle::Car", SymbolKind::PartDef, 0),
            make_symbol("engine", "Vehicle::Car::engine", SymbolKind::PartUsage, 0),
        ];
        
        index.add_file(FileId::new(0), symbols);
        
        assert_eq!(index.len(), 3);
        assert!(index.lookup_qualified("Vehicle::Car").is_some());
        assert!(index.lookup_qualified("Vehicle::Car::engine").is_some());
        assert!(index.lookup_definition("Vehicle::Car").is_some());
        assert!(index.lookup_definition("Vehicle::Car::engine").is_none()); // Usage, not def
    }

    #[test]
    fn test_symbol_index_remove_file() {
        let mut index = SymbolIndex::new();
        
        index.add_file(FileId::new(0), vec![
            make_symbol("A", "A", SymbolKind::PartDef, 0),
        ]);
        index.add_file(FileId::new(1), vec![
            make_symbol("B", "B", SymbolKind::PartDef, 1),
        ]);
        
        assert_eq!(index.len(), 2);
        
        index.remove_file(FileId::new(0));
        
        assert_eq!(index.len(), 1);
        assert!(index.lookup_qualified("A").is_none());
        assert!(index.lookup_qualified("B").is_some());
    }

    #[test]
    fn test_resolver_qualified_name() {
        let mut index = SymbolIndex::new();
        index.add_file(FileId::new(0), vec![
            make_symbol("Car", "Vehicle::Car", SymbolKind::PartDef, 0),
        ]);
        
        let resolver = Resolver::new(&index);
        let result = resolver.resolve("Vehicle::Car");
        
        assert!(result.is_found());
        assert_eq!(result.symbol().unwrap().name.as_ref(), "Car");
    }

    #[test]
    fn test_resolver_with_scope() {
        let mut index = SymbolIndex::new();
        index.add_file(FileId::new(0), vec![
            make_symbol("Car", "Vehicle::Car", SymbolKind::PartDef, 0),
            make_symbol("engine", "Vehicle::Car::engine", SymbolKind::PartUsage, 0),
        ]);
        index.ensure_visibility_maps();
        
        let resolver = Resolver::new(&index).with_scope("Vehicle::Car");
        let result = resolver.resolve("engine");
        
        assert!(result.is_found());
    }

    #[test]
    fn test_resolver_with_visibility_maps() {
        let mut index = SymbolIndex::new();
        // Create a package ISQ with Real defined inside
        index.add_file(FileId::new(0), vec![
            make_symbol("ISQ", "ISQ", SymbolKind::Package, 0),
            make_symbol("Real", "ISQ::Real", SymbolKind::AttributeDef, 0),
        ]);
        // Create an import from another scope
        let mut import_sym = make_symbol("ISQ::*", "TestPkg::import:ISQ::*", SymbolKind::Import, 1);
        import_sym.is_public = false;
        index.add_file(FileId::new(1), vec![
            make_symbol("TestPkg", "TestPkg", SymbolKind::Package, 1),
            import_sym,
        ]);
        index.ensure_visibility_maps();
        
        // Resolver from TestPkg should find Real via import
        let resolver = Resolver::new(&index).with_scope("TestPkg");
        let result = resolver.resolve("Real");
        
        assert!(result.is_found());
        assert_eq!(result.symbol().unwrap().qualified_name.as_ref(), "ISQ::Real");
    }

    #[test]
    fn test_symbol_kind_is_definition() {
        assert!(SymbolKind::PartDef.is_definition());
        assert!(SymbolKind::ActionDef.is_definition());
        assert!(!SymbolKind::PartUsage.is_definition());
        assert!(!SymbolKind::Import.is_definition());
    }

    #[test]
    fn test_symbol_kind_is_usage() {
        assert!(SymbolKind::PartUsage.is_usage());
        assert!(SymbolKind::ActionUsage.is_usage());
        assert!(!SymbolKind::PartDef.is_usage());
        assert!(!SymbolKind::Package.is_usage());
    }
}
