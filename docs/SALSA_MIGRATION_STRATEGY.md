# Salsa Migration Strategy for syster-base

## Executive Summary

This document outlines a comprehensive strategy to migrate syster-base from its current eager/imperative architecture to a query-based architecture using the [Salsa](https://github.com/salsa-rs/salsa) crate. This migration will:

- **Improve incrementality**: Only re-compute what changes
- **Simplify code**: Replace manual invalidation with declarative dependencies
- **Enable parallelism**: Salsa's memoization allows safe concurrent queries
- **Increase maintainability**: Clear separation of concerns via query interfaces

---

## Part 1: Current Architecture Analysis

### 1.1 Component Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         LSP Server                               │
│  (syster-lsp/crates/syster-lsp/src/server/document.rs)          │
└─────────────────────────┬───────────────────────────────────────┘
                          │ parse_into_workspace()
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Workspace<SysMLFile>                          │
│  (semantic/workspace/core.rs)                                    │
│  - files: HashMap<PathBuf, WorkspaceFile<T>>                    │
│  - symbol_table: SymbolTable (MUTABLE)                          │
│  - reference_index: ReferenceIndex (MUTABLE)                    │
└─────────────────────────┬───────────────────────────────────────┘
                          │ populate_affected()
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│                    SysmlAdapter::populate()                      │
│  (semantic/adapters/sysml/population.rs)                        │
│  - Clears symbols for file                                      │
│  - Walks AST eagerly                                            │
│  - MUTATES symbol_table.insert()                                │
│  - MUTATES reference_index.add_reference()                      │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 Current Data Flow

```
1. TEXT INPUT
   └─► pest::Parser::parse(Rule::file, content)
       └─► Pairs<Rule>

2. AST CONSTRUCTION  
   └─► syntax/sysml/ast/parsers.rs::parse_file()
       └─► SysMLFile { definitions: Vec<Definition> }
``````
3. SYMBOL POPULATION (eager, mutable)
   └─► SysmlAdapter::populate(&mut symbol_table, &ast)
       └─► visit_definition() → symbol_table.insert()
       └─► visit_usage() → symbol_table.insert()
       └─► ... mutate reference_index ...

4. NAME RESOLUTION (on-demand, but re-traverses)
   └─► Resolver::resolve_name()
       └─► symbol_table.lookup()
```

### 1.3 Key Files to Transform

| Current Module | Purpose | Migration Priority |
|----------------|---------|-------------------|
| `semantic/symbol_table/table.rs` | Mutable symbol storage | HIGH - becomes derived |
| `semantic/workspace/core.rs` | Workspace state | HIGH - holds database |
| `semantic/workspace/population.rs` | Eager population | HIGH - becomes queries |
| `semantic/adapters/sysml/population.rs` | AST → symbols | HIGH - becomes queries |
| `semantic/adapters/sysml/visitors.rs` | AST traversal | MEDIUM - refactor |
| `semantic/resolver/name_resolver.rs` | Name lookup | MEDIUM - becomes query |
| `semantic/graphs/reference_index.rs` | Reference storage | HIGH - becomes derived |
| `syntax/sysml/parser.rs` | Parsing | LOW - input query |

### 1.4 Mutation Points (must become queries)

1. **SymbolTable mutations**:
   - `insert()` - called during population
   - `clear_file_symbols()` - called on re-parse
   - `enter_scope()` / `exit_scope()` - scope management
   - `add_import()` / `resolve_imports()` - import handling

2. **ReferenceIndex mutations**:
   - `add_reference()` - during population
   - `clear_file_references()` - on re-parse

3. **Workspace mutations**:
   - `add_file()` / `update_file()` - file tracking
   - `populate_affected()` - triggers re-population

---

## Part 2: Target Architecture

### 2.1 Salsa Concepts

- **Input**: Data you set explicitly (file contents, config)
- **Tracked**: Data with identity that Salsa tracks
- **Query/Function**: Derived data computed from inputs/other queries
- **Database**: Container for all queries and memoization

### 2.2 Query Design

```rust
// ═══════════════════════════════════════════════════════════════
// INPUT QUERIES (set explicitly)
// ═══════════════════════════════════════════════════════════════

/// The raw text content of a file
#[salsa::input]
pub fn file_text(db: &dyn Db, file: FileId) -> Arc<String>;

/// List of all files in the workspace
#[salsa::input]  
pub fn all_files(db: &dyn Db) -> Arc<Vec<FileId>>;

/// Standard library file paths
#[salsa::input]
pub fn stdlib_files(db: &dyn Db) -> Arc<Vec<FileId>>;

// ═══════════════════════════════════════════════════════════════
// DERIVED QUERIES (computed, memoized)
// ═══════════════════════════════════════════════════════════════

/// Parse a file into AST (errors included in result)
#[salsa::tracked]
pub fn parse_file(db: &dyn Db, file: FileId) -> ParseResult;

/// Extract symbols defined in a single file
#[salsa::tracked]
pub fn file_symbols(db: &dyn Db, file: FileId) -> Arc<Vec<Symbol>>;

/// Extract imports declared in a file
#[salsa::tracked]  
pub fn file_imports(db: &dyn Db, file: FileId) -> Arc<Vec<Import>>;

/// Build the full symbol table (aggregates all file_symbols)
#[salsa::tracked]
pub fn symbol_table(db: &dyn Db) -> Arc<SymbolTable>;

/// Resolve a name in a given scope
#[salsa::tracked]
pub fn resolve_name(
    db: &dyn Db, 
    scope: ScopeId, 
    name: String
) -> Option<SymbolId>;

/// Get all references in a file
#[salsa::tracked]
pub fn file_references(db: &dyn Db, file: FileId) -> Arc<Vec<Reference>>;

/// Build the full reference index
#[salsa::tracked]
pub fn reference_index(db: &dyn Db) -> Arc<ReferenceIndex>;

/// Get diagnostics for a file
#[salsa::tracked]
pub fn file_diagnostics(db: &dyn Db, file: FileId) -> Arc<Vec<Diagnostic>>;
```

### 2.3 Dependency Graph

```
file_text(file)              ← INPUT
    │
    ▼
parse_file(file)             ← file-local, cheap to recompute
    │
    ├─────────────────┐
    ▼                 ▼
file_symbols(file)   file_imports(file)
    │                     │
    └────────┬────────────┘
             ▼
      symbol_table()         ← aggregates all files
             │
             ▼
      resolve_name(scope, name)
             │
             ▼
      file_references(file)
             │
             ▼
      reference_index()
             │
             ▼
      file_diagnostics(file)
```

### 2.4 Database Structure

```rust
#[salsa::db]
pub trait Db: salsa::Database {
    // Inputs
    fn file_text(&self, file: FileId) -> Arc<String>;
    fn all_files(&self) -> Arc<Vec<FileId>>;
    fn stdlib_files(&self) -> Arc<Vec<FileId>>;
}

#[salsa::db]
#[derive(Default)]
pub struct SysterDatabase {
    storage: salsa::Storage<Self>,
    // Input storage
    files: HashMap<FileId, Arc<String>>,
}

impl salsa::Database for SysterDatabase {}

impl Db for SysterDatabase {
    fn file_text(&self, file: FileId) -> Arc<String> {
        self.files.get(&file).cloned().unwrap_or_default()
    }
    // ...
}
```

---

## Part 3: Migration Phases

### Phase 0: Foundation (1-2 weeks)

**Goal**: Add Salsa dependency, create database skeleton, no behavior changes.

**Tasks**:
1. Add `salsa = "0.18"` to `Cargo.toml`
2. Create `src/database/mod.rs` with:
   - `FileId` interned identifier
   - `Db` trait definition
   - `SysterDatabase` struct
3. Create input setters for file_text
4. Write integration test that creates database, sets file text

**Deliverable**: Compiles, all existing tests pass, new database module exists but unused.

**Files to create**:
```
src/
  database/
    mod.rs        # Db trait, SysterDatabase
    inputs.rs     # Input query implementations
    ids.rs        # FileId, ScopeId, SymbolId interning
```

---

### Phase 1: Parse Query (1 week)

**Goal**: `parse_file` query replaces direct parser calls.

**Tasks**:
1. Create `parse_file` tracked function
2. Return `ParseResult { ast: Option<SysMLFile>, errors: Vec<ParseError> }`
3. Modify `Workspace::add_file` to set input, call query
4. Keep existing population path working (reads from query result)

**Key changes**:
```rust
// Before (direct call):
let result = parse_with_result(text, Some(path));
workspace.add_file(path, result.content);

// After (via query):
db.set_file_text(file_id, Arc::new(text));
let result = parse_file(&db, file_id);  // memoized!
workspace.add_file(path, result.ast);
```

**Deliverable**: Parsing is memoized. Editing a file only re-parses that file.

---

### Phase 2: Symbol Extraction Query (2 weeks)

**Goal**: Replace `SysmlAdapter::populate()` with `file_symbols()` query.

**Tasks**:
1. Create `file_symbols(db, file) -> Arc<Vec<Symbol>>` query
2. Refactor `AstVisitor` to be pure (returns symbols, doesn't mutate)
3. Create `symbol_table(db) -> Arc<SymbolTable>` that aggregates
4. Workspace reads from `symbol_table()` query instead of owning

**Key refactoring**:
```rust
// Before (mutable visitor):
impl AstVisitor for SysmlPopulator<'_> {
    fn visit_definition(&mut self, def: &Definition) {
        self.symbol_table.insert(...);  // MUTATION
    }
}

// After (pure function):
fn extract_symbols(ast: &SysMLFile) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for def in &ast.definitions {
        symbols.push(symbol_from_definition(def));
    }
    symbols  // RETURN, no mutation
}

#[salsa::tracked]
fn file_symbols(db: &dyn Db, file: FileId) -> Arc<Vec<Symbol>> {
    let ast = parse_file(db, file);
    Arc::new(extract_symbols(&ast))
}
```

**Deliverable**: Symbol extraction is per-file and memoized.

---

### Phase 3: Import Resolution Query (1-2 weeks)

**Goal**: Import resolution becomes query-based.

**Tasks**:
1. Create `file_imports(db, file) -> Arc<Vec<Import>>` query
2. Create `resolve_import(db, import) -> Option<SymbolId>` query
3. Create `visible_symbols(db, file) -> Arc<Vec<SymbolId>>` query
4. Remove `imports_by_file` HashMap from SymbolTable

**Deliverable**: Imports are resolved lazily, cached per-file.

---

### Phase 4: Reference Index Query (1-2 weeks)

**Goal**: Reference tracking becomes query-based.

**Tasks**:
1. Create `file_references(db, file) -> Arc<Vec<Reference>>` query
2. Create `reference_index(db) -> Arc<ReferenceIndex>` aggregator
3. Remove `ReferenceIndex` mutations from population
4. LSP "find references" uses query

**Deliverable**: Reference index is derived, not manually maintained.

---

### Phase 5: Name Resolution Query (2 weeks)

**Goal**: `resolve_name` becomes a memoized query.

**Tasks**:
1. Create `resolve_name(db, scope, name) -> Option<SymbolId>` query
2. Create `scope_symbols(db, scope) -> Arc<Vec<SymbolId>>` query  
3. Refactor `Resolver` to use queries internally
4. Handle cycles gracefully (Salsa handles this)

**Deliverable**: Name resolution is cached per (scope, name) pair.

---

### Phase 6: Cleanup & Optimization (1-2 weeks)

**Goal**: Remove old infrastructure, optimize hot paths.

**Tasks**:
1. Remove `populate_affected()` and eager population code
2. Remove `clear_file_symbols()` / `clear_file_references()`
3. Remove manual `HashMap` indices that Salsa now provides
4. Profile and optimize query granularity
5. Add parallel query execution where beneficial

**Deliverable**: Clean codebase, measurably faster incremental updates.

---

## Part 4: Detailed Implementation Guide

### 4.1 Phase 0: FileId Interning

```rust
// src/database/ids.rs

use std::path::PathBuf;

/// Interned file identifier for efficient comparison and hashing
#[salsa::interned]
pub struct FileId {
    #[return_ref]
    pub path: PathBuf,
}

/// Interned scope identifier
#[salsa::interned]
pub struct ScopeId {
    pub file: FileId,
    pub local_id: u32,  // scope index within file
}

/// Symbol identifier (combines file + local index)
#[salsa::interned]
pub struct SymbolId {
    pub file: FileId,
    pub local_id: u32,  // symbol index within file
}
```

### 4.2 Phase 0: Database Trait

```rust
// src/database/mod.rs

mod ids;
mod inputs;

pub use ids::{FileId, ScopeId, SymbolId};

use std::sync::Arc;

#[salsa::db]
pub trait Db: salsa::Database {
    // === INPUTS ===
    
    /// Get the text content of a file
    fn file_text(&self, file: FileId) -> Arc<String>;
    
    /// Get all file IDs in the workspace
    fn all_files(&self) -> Arc<Vec<FileId>>;
    
    /// Whether stdlib is loaded
    fn stdlib_loaded(&self) -> bool;
}

#[salsa::db]
#[derive(Default)]
pub struct SysterDatabase {
    storage: salsa::Storage<Self>,
}

impl salsa::Database for SysterDatabase {}
```

### 4.3 Phase 1: Parse Query

```rust
// src/database/queries/parse.rs

use crate::database::{Db, FileId};
use crate::syntax::sysml::ast::SysMLFile;
use crate::syntax::sysml::parser::parse_with_result;
use std::sync::Arc;

/// Result of parsing a file
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseResult {
    pub ast: Option<Arc<SysMLFile>>,
    pub errors: Arc<Vec<ParseError>>,
}

/// Parse a file and return AST + errors
#[salsa::tracked]
pub fn parse_file(db: &dyn Db, file: FileId) -> ParseResult {
    let text = db.file_text(file);
    let path = file.path(db);
    
    let result = parse_with_result(&text, Some(&path));
    
    ParseResult {
        ast: result.content.map(Arc::new),
        errors: Arc::new(result.errors),
    }
}
```

### 4.4 Phase 2: Symbol Extraction

```rust
// src/database/queries/symbols.rs

use crate::database::{Db, FileId, SymbolId};
use crate::semantic::symbol_table::Symbol;
use std::sync::Arc;

/// Extract all symbols defined in a file
#[salsa::tracked]
pub fn file_symbols(db: &dyn Db, file: FileId) -> Arc<Vec<Symbol>> {
    let parse_result = parse_file(db, file);
    
    let Some(ast) = &parse_result.ast else {
        return Arc::new(vec![]);
    };
    
    let symbols = extract_symbols_from_ast(ast, file);
    Arc::new(symbols)
}

/// Pure function: AST → Symbols (no mutation)
fn extract_symbols_from_ast(ast: &SysMLFile, file: FileId) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    
    for definition in &ast.definitions {
        if let Some(sym) = symbol_from_definition(definition, file) {
            symbols.push(sym);
        }
        
        // Recurse into nested definitions
        symbols.extend(extract_nested_symbols(definition, file));
    }
    
    symbols
}

/// Aggregate all file symbols into a symbol table
#[salsa::tracked]
pub fn symbol_table(db: &dyn Db) -> Arc<SymbolTable> {
    let files = db.all_files();
    let mut table = SymbolTable::new();
    
    for &file in files.iter() {
        let symbols = file_symbols(db, file);
        for symbol in symbols.iter() {
            table.add_symbol(symbol.clone());
        }
    }
    
    Arc::new(table)
}
```

### 4.5 Workspace Integration

```rust
// src/semantic/workspace/core.rs (modified)

use crate::database::{Db, FileId, SysterDatabase};

/// A workspace now wraps a Salsa database
pub struct Workspace {
    db: SysterDatabase,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            db: SysterDatabase::default(),
        }
    }
    
    /// Add or update a file
    pub fn set_file(&mut self, path: PathBuf, content: String) {
        let file = FileId::new(&self.db, path);
        self.db.set_file_text(file, Arc::new(content));
        // No eager population! Queries will compute on demand.
    }
    
    /// Get symbols (lazy, memoized)
    pub fn symbols(&self) -> Arc<SymbolTable> {
        symbol_table(&self.db)
    }
    
    /// Get parse errors for a file
    pub fn parse_errors(&self, path: &Path) -> Vec<ParseError> {
        let file = FileId::new(&self.db, path.to_path_buf());
        let result = parse_file(&self.db, file);
        result.errors.to_vec()
    }
}
```

---

## Part 5: Testing Strategy

### 5.1 Unit Tests for Each Query

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_file_memoization() {
        let mut db = SysterDatabase::default();
        let file = FileId::new(&db, "test.sysml".into());
        
        db.set_file_text(file, Arc::new("part def A;".into()));
        
        // First call computes
        let result1 = parse_file(&db, file);
        
        // Second call returns cached
        let result2 = parse_file(&db, file);
        
        assert!(Arc::ptr_eq(&result1.ast.unwrap(), &result2.ast.unwrap()));
    }
    
    #[test]
    fn test_parse_invalidation() {
        let mut db = SysterDatabase::default();
        let file = FileId::new(&db, "test.sysml".into());
        
        db.set_file_text(file, Arc::new("part def A;".into()));
        let result1 = parse_file(&db, file);
        
        // Change input
        db.set_file_text(file, Arc::new("part def B;".into()));
        let result2 = parse_file(&db, file);
        
        // Should be different
        assert_ne!(result1.ast, result2.ast);
    }
}
```

### 5.2 Integration Tests

```rust
#[test]
fn test_incremental_symbol_update() {
    let mut workspace = Workspace::new();
    
    // Add file A with reference to B
    workspace.set_file("a.sysml".into(), "part def A : B;".into());
    workspace.set_file("b.sysml".into(), "part def B;".into());
    
    let symbols1 = workspace.symbols();
    assert!(symbols1.lookup("A").is_some());
    assert!(symbols1.lookup("B").is_some());
    
    // Modify B - only B should recompute
    workspace.set_file("b.sysml".into(), "part def B { attribute x; }".into());
    
    let symbols2 = workspace.symbols();
    // A's symbols should be same Arc (not recomputed)
    // B's symbols should be new
}
```

### 5.3 Regression Tests

Keep all existing tests in `examples/test_*.rs` passing throughout migration:

```bash
# Run before each PR
cargo test --examples
cargo test --lib
```

---

## Part 6: Risk Mitigation

### 6.1 Identified Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Salsa API changes | Medium | High | Pin to specific version, monitor releases |
| Performance regression | Medium | Medium | Benchmark before/after each phase |
| Cyclic dependencies | Low | High | Salsa handles cycles; design acyclic queries |
| Breaking LSP integration | High | High | Keep facade API stable |
| Memory usage increase | Medium | Medium | Profile, use Arc for sharing |

### 6.2 Rollback Strategy

Each phase should be behind a feature flag initially:

```toml
# Cargo.toml
[features]
default = []
salsa-queries = ["salsa"]
```

```rust
#[cfg(feature = "salsa-queries")]
pub fn symbols(&self) -> Arc<SymbolTable> {
    symbol_table(&self.db)
}

#[cfg(not(feature = "salsa-queries"))]
pub fn symbols(&self) -> &SymbolTable {
    &self.symbol_table  // old path
}
```

### 6.3 Compatibility Layer

During migration, keep `Workspace` API identical:

```rust
impl Workspace {
    // Old API (keep working)
    pub fn get_symbol(&self, name: &str) -> Option<&Symbol>
    pub fn find_references(&self, symbol: SymbolId) -> Vec<Reference>
    pub fn get_diagnostics(&self, file: &Path) -> Vec<Diagnostic>
    
    // New API (add incrementally)  
    pub fn db(&self) -> &dyn Db  // expose database for advanced queries
}
```

---

## Part 7: Performance Benchmarks

### 7.1 Key Metrics to Track

1. **Cold parse time**: Time to parse entire workspace from scratch
2. **Hot parse time**: Time to re-parse single file after edit
3. **Symbol lookup time**: Time to resolve a name
4. **Memory usage**: Peak memory with N files loaded
5. **Reference search time**: Time to find all references to a symbol

### 7.2 Benchmark Harness

```rust
// benches/incremental.rs

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_incremental_edit(c: &mut Criterion) {
    let mut workspace = Workspace::new();
    
    // Load 100 files
    for i in 0..100 {
        workspace.set_file(
            format!("file{i}.sysml").into(),
            format!("part def Part{i};")
        );
    }
    
    // Warm up
    let _ = workspace.symbols();
    
    c.bench_function("edit_single_file", |b| {
        b.iter(|| {
            workspace.set_file("file50.sysml".into(), "part def Modified;".into());
            let _ = workspace.symbols();
        })
    });
}

criterion_group!(benches, bench_incremental_edit);
criterion_main!(benches);
```

### 7.3 Expected Improvements

| Operation | Before (Eager) | After (Salsa) | Improvement |
|-----------|----------------|---------------|-------------|
| Edit single file | O(n) re-population | O(1) file re-parse | 10-100x |
| Add new file | O(n) re-population | O(1) file parse | 10-100x |
| Name resolution | O(1) lookup | O(1) cached query | Same |
| Cold start | O(n) | O(n) | Same |

---

## Part 8: Timeline & Milestones

### Estimated Timeline: 10-14 weeks

```
Week 1-2:   Phase 0 - Foundation
Week 3:     Phase 1 - Parse Query
Week 4-5:   Phase 2 - Symbol Extraction
Week 6-7:   Phase 3 - Import Resolution  
Week 8-9:   Phase 4 - Reference Index
Week 10-11: Phase 5 - Name Resolution
Week 12-14: Phase 6 - Cleanup & Optimization
```

### Milestones

1. **M1 (Week 2)**: Database compiles, feature-flagged, tests pass
2. **M2 (Week 5)**: Parsing and symbols via queries, 50% faster incremental
3. **M3 (Week 9)**: Full query system, old code deprecated
4. **M4 (Week 14)**: Migration complete, old code removed, benchmarks met

---

## Part 9: Code Examples Reference

### Example: Before vs After for Complete Flow

**BEFORE (Eager/Mutable)**:
```rust
// In document.rs
pub fn parse_into_workspace(workspace: &mut Workspace, path: PathBuf, text: &str) {
    let result = parse_with_result(text, Some(&path));
    
    if let Some(file) = result.content {
        workspace.update_file(path.clone(), file.clone());
        workspace.populate_affected(&[path]);  // EAGER: walks all symbols
    }
}

// In population.rs
pub fn populate_affected(&mut self, files: &[PathBuf]) {
    for path in files {
        self.symbol_table.clear_file_symbols(path);  // MUTATION
        if let Some(file) = self.files.get(path) {
            SysmlAdapter::populate(&mut self.symbol_table, &file.content);  // MUTATION
        }
    }
}
```

**AFTER (Query/Declarative)**:
```rust
// In document.rs
pub fn update_file(workspace: &mut Workspace, path: PathBuf, text: &str) {
    workspace.set_file(path, text.to_string());
    // That's it! No eager population. Queries compute on demand.
}

// In queries/symbols.rs
#[salsa::tracked]
pub fn file_symbols(db: &dyn Db, file: FileId) -> Arc<Vec<Symbol>> {
    let result = parse_file(db, file);  // Automatically cached
    extract_symbols(&result.ast)        // Pure function
}

// When LSP needs symbols:
let symbols = file_symbols(&db, file);  // Returns cached or computes
```

---

## Part 10: Resources

### Documentation
- [Salsa Book](https://salsa-rs.github.io/salsa/)
- [rust-analyzer Architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md)
- [Roslyn Compiler Architecture](https://github.com/dotnet/roslyn/wiki/Roslyn-Overview)

### Reference Implementations
- rust-analyzer: Full Salsa-based language server
- chalk: Rust trait solver using Salsa
- mun-lang: Game scripting language with Salsa

### Crate Versions
```toml
[dependencies]
salsa = "0.18"  # Or latest stable
```

---

## Appendix A: File Checklist

### Files to Create
- [ ] `src/database/mod.rs`
- [ ] `src/database/ids.rs`
- [ ] `src/database/inputs.rs`
- [ ] `src/database/queries/mod.rs`
- [ ] `src/database/queries/parse.rs`
- [ ] `src/database/queries/symbols.rs`
- [ ] `src/database/queries/imports.rs`
- [ ] `src/database/queries/references.rs`
- [ ] `src/database/queries/resolve.rs`
- [ ] `benches/incremental.rs`

### Files to Modify
- [ ] `Cargo.toml` - add salsa dependency
- [ ] `src/lib.rs` - export database module
- [ ] `src/semantic/workspace/core.rs` - wrap database
- [ ] `src/semantic/adapters/sysml/population.rs` - convert to pure functions
- [ ] `src/semantic/adapters/sysml/visitors.rs` - make stateless
- [ ] `src/semantic/symbol_table/table.rs` - simplify (derived state)
- [ ] `src/semantic/resolver/name_resolver.rs` - use queries

### Files to Eventually Remove
- [ ] `src/semantic/workspace/population.rs` (eager population)
- [ ] `src/semantic/workspace/populator.rs` (manual invalidation)
- [ ] Various `clear_*` methods on SymbolTable/ReferenceIndex

---

## Appendix B: Quick Start Commands

```bash
# Add Salsa dependency
cargo add salsa@0.18

# Run tests during migration
cargo test --features salsa-queries

# Benchmark comparison
cargo bench --features salsa-queries

# Check compilation without Salsa (fallback)
cargo check --no-default-features
```

---

*Document Version: 1.0*  
*Created: $(date)*  
*Author: GitHub Copilot*
