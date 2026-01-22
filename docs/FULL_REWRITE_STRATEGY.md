# Full Rewrite Strategy for syster-base

## The Problem

The current codebase is a mess of tangled concerns:
- Parser, AST, semantic analysis, and LSP are all intertwined
- Mutable state scattered everywhere
- No clear layering or dependency direction
- Impossible to reason about incrementality
- Adding features requires touching 10 files

**We're not migrating. We're starting over.**

---

## Current Progress (January 2026)

### What's Built

We have implemented the foundational layers:

```
syster-base/src/
├── base/           ✅ FileId, Span primitives
├── hir/            ✅ SymbolIndex, HirSymbol, TypeRef, SymbolKind
│   ├── mod.rs
│   ├── symbols.rs      # HirSymbol with type_refs
│   ├── resolve.rs      # SymbolIndex with lookup methods
│   └── diagnostics.rs  # Diagnostic types
├── ide/            ✅ Pure IDE functions
│   ├── hover.rs        # hover()
│   ├── goto.rs         # goto_definition()
│   ├── references.rs   # find_references()
│   ├── completion.rs   # completions()
│   ├── symbols.rs      # workspace_symbols(), document_symbols()
│   ├── document_links.rs # document_links()
│   └── bridge.rs       # symbol_table_to_index() converter
```

### LSP Handler Migration Status

**Using new IDE layer (✅ migrated):**
- `hover.rs` → `ide::hover()`
- `definition.rs` → `ide::goto_definition()`
- `references.rs` → `ide::find_references()`
- `completion.rs` → `ide::completions()`
- `document_symbols.rs` → `ide::document_symbols()`
- `workspace_symbols.rs` → `ide::workspace_symbols()`
- `document_links.rs` → `ide::document_links()`
- `rename.rs` → uses `ide::find_references()`
- `code_lens.rs` → uses SymbolIndex directly
- `diagram.rs` → uses SymbolIndex directly
- `position.rs` → uses SymbolIndex directly

**Still using legacy Workspace (❌ needs migration):**
- `folding_ranges.rs` → needs `ide::folding_ranges()`
- `selection_range.rs` → needs `ide::selection_ranges()`
- `inlay_hints.rs` → needs `ide::inlay_hints()`
- `semantic_tokens.rs` → needs `ide::semantic_tokens()`

### Current Architecture Gap

We have a **bridge** pattern but not the **full query architecture**:

```
Current (hybrid - problematic):
┌─────────────────────────────────────────────────────────────────┐
│  syster-lsp                                                     │
│  ├── Uses ide::* for symbol-based features                     │
│  ├── Uses workspace.files() for AST-based features ❌          │
│  └── Calls rebuild_symbol_index() manually ❌                  │
└─────────────────────────────────────────────────────────────────┘

Target (query-based):
┌─────────────────────────────────────────────────────────────────┐
│  syster-lsp                                                     │
│  └── Only uses ide::Analysis (snapshot)                        │
│      └── All features go through queries                       │
└─────────────────────────────────────────────────────────────────┘
```

---

## Immediate Next Steps

### Step 1: Add AST-based IDE Functions

Create IDE functions that internally use the existing semantic layer:

```rust
// ide/folding.rs
pub fn folding_ranges(
    file: FileId,
    syntax_file: &SyntaxFile,  // temporary: pass AST directly
) -> Vec<FoldingRange> {
    // Wraps existing extract_folding_ranges()
}

// ide/selection.rs  
pub fn selection_ranges(
    file: FileId,
    syntax_file: &SyntaxFile,
    positions: &[Position],
) -> Vec<SelectionRange> {
    // Wraps existing find_selection_spans()
}

// ide/inlay_hints.rs
pub fn inlay_hints(
    index: &SymbolIndex,
    file: FileId,
    syntax_file: &SyntaxFile,
    range: Option<(Position, Position)>,
) -> Vec<InlayHint> {
    // Wraps existing extract_inlay_hints()
}

// ide/semantic_tokens.rs
pub fn semantic_tokens(
    file: FileId,
    syntax_file: &SyntaxFile,
) -> Vec<SemanticToken> {
    // Wraps existing SemanticTokenCollector
}
```

### Step 2: Update LSP Handlers

Migrate remaining handlers to use IDE functions:

```rust
// Before (folding_ranges.rs):
let workspace_file = self.workspace.files().get(file_path)?;
extract_folding_ranges(workspace_file.content())

// After:
let syntax_file = self.get_syntax_file(file_path)?;
ide::folding_ranges(file_id, &syntax_file)
```

### Step 3: Create AnalysisHost/Analysis Pattern ✅ COMPLETE

We now have `ide/analysis.rs` with `AnalysisHost` and `Analysis`:

```rust
// ide/analysis.rs
pub struct AnalysisHost {
    workspace: Workspace<SyntaxFile>,
    symbol_index: SymbolIndex,
    file_id_map: HashMap<String, FileId>,
    file_path_map: HashMap<FileId, String>,
    index_dirty: bool,
}

impl AnalysisHost {
    pub fn set_file_content(&mut self, path: &str, content: &str) -> Vec<ParseError>;
    pub fn remove_file(&mut self, path: &str);
    pub fn analysis(&mut self) -> Analysis<'_>;  // Rebuilds index if dirty
}

pub struct Analysis<'a> {
    // Immutable snapshot for consistent queries
}

impl Analysis<'_> {
    // Symbol-based features
    pub fn hover(&self, file: FileId, line: u32, col: u32) -> Option<HoverResult>;
    pub fn goto_definition(&self, file: FileId, line: u32, col: u32) -> GotoResult;
    pub fn find_references(&self, file: FileId, line: u32, col: u32, include_declaration: bool) -> ReferenceResult;
    pub fn completions(&self, file: FileId, line: u32, col: u32, trigger: Option<char>) -> Vec<CompletionItem>;
    pub fn document_symbols(&self, file: FileId) -> Vec<SymbolInfo>;
    pub fn workspace_symbols(&self, query: Option<&str>) -> Vec<SymbolInfo>;
    pub fn document_links(&self, file: FileId) -> Vec<DocumentLink>;
    
    // AST-based features
    pub fn folding_ranges(&self, file: FileId) -> Vec<FoldingRange>;
    pub fn selection_ranges(&self, file: FileId, line: u32, col: u32) -> Vec<SelectionRange>;
    pub fn inlay_hints(&self, file: FileId, range: Option<(u32, u32, u32, u32)>) -> Vec<InlayHint>;
    pub fn semantic_tokens(&self, file: FileId) -> Vec<SemanticToken>;
}
```

### Step 4: Migrate LspServer to Use AnalysisHost

```rust
// Before (core.rs):
pub struct LspServer {
    workspace: Workspace<SyntaxFile>,
    symbol_index: SymbolIndex,
    file_id_map: HashMap<String, FileId>,
    // ... scattered state
}

// After:
pub struct LspServer {
    analysis: AnalysisHost,
    document_texts: HashMap<PathBuf, String>,  // editor state only
    parse_errors: HashMap<PathBuf, Vec<ParseError>>,
}
```

### Step 5: Remove Legacy Types

Once all handlers migrated:
- Remove `Workspace` usage from LSP
- Remove `resolver()` method
- Remove bridge converter (data lives in AnalysisHost)
- Symbol table only used internally by AnalysisHost

---

## File-by-File Migration Plan

### 1. `folding_ranges.rs`

**Current:**
```rust
let workspace_file = self.workspace.files().get(file_path)?;
let ranges = extract_folding_ranges(workspace_file.content());
```

**Target:**
```rust
let analysis = self.analysis.analysis();
let ranges = analysis.folding_ranges(file_id);
```

**Steps:**
1. Create `ide/folding.rs` with `folding_ranges()` function
2. Add `folding_ranges()` method to `Analysis`
3. Update LSP handler to use `analysis.folding_ranges()`
4. Remove `workspace.files()` access

### 2. `selection_range.rs`

**Current:**
```rust
let workspace_file = self.workspace.files().get(file_path)?;
let spans = find_selection_spans(workspace_file.content(), core_pos);
```

**Target:**
```rust
let analysis = self.analysis.analysis();
let ranges = analysis.selection_ranges(file_id, &positions);
```

### 3. `inlay_hints.rs`

**Current:**
```rust
let workspace_file = self.workspace.files().get(&path)?;
let hints = extract_inlay_hints(
    workspace_file.content(),
    self.workspace.symbol_table(),
    range,
);
```

**Target:**
```rust
let analysis = self.analysis.analysis();
let hints = analysis.inlay_hints(file_id, range);
```

### 4. `semantic_tokens.rs`

**Current:**
```rust
let tokens = SemanticTokenCollector::collect_from_workspace(&self.workspace, &path_str);
```

**Target:**
```rust
let analysis = self.analysis.analysis();
let tokens = analysis.semantic_tokens(file_id);
```

---

## Migration Order

1. **Create IDE functions** (no LSP changes yet)
   - `ide/folding.rs`
   - `ide/selection.rs`
   - `ide/inlay_hints.rs`
   - `ide/semantic_tokens.rs`

2. **Create AnalysisHost** in `ide/analysis.rs`
   - Holds SymbolIndex + file contents
   - Provides Analysis snapshot

3. **Migrate LspServer to AnalysisHost**
   - Replace `workspace` + `symbol_index` fields
   - Update `rebuild_symbol_index()` to `analysis.apply_change()`

4. **Update remaining LSP handlers**
   - One at a time, test after each

5. **Remove legacy code**
   - Remove `Workspace` from LspServer
   - Remove bridge converter
   - Clean up unused imports

---

## Target Architecture: Layered Modules

We restructure **within syster-base** using strict module layering:

```
┌─────────────────────────────────────────────────────────────────┐
│                        syster-lsp (separate crate)              │
│  (LSP protocol, JSON-RPC, VS Code specifics)                   │
│  ONLY knows about: Analysis results, positions, ranges         │
└─────────────────────────┬───────────────────────────────────────┘
                          │ uses syster-base::ide
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  syster-base/                                                   │
│  ├── ide/          ← IDE features (completion, hover, goto)    │
│  ├── hir/          ← Semantic model (Salsa queries)            │
│  ├── ast/          ← Typed syntax tree wrappers                │
│  ├── parser/       ← Lexer + hand-written parser               │
│  └── base/         ← Primitives (FileId, Span, Name)           │
└─────────────────────────────────────────────────────────────────┘
```

**Module dependency rule: ide → hir → ast → parser → base**

---

## Module Breakdown

### 1. `syster-base` — Foundation

Shared types with ZERO language-specific logic.

```
syster-base/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── file_id.rs      # Interned file identifiers
    ├── span.rs         # TextRange, TextSize, LineCol
    ├── intern.rs       # String interning (names, paths)
    └── cancellation.rs # Cooperative cancellation token
```

```rust
// file_id.rs
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FileId(pub u32);

// span.rs
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TextRange {
    pub start: TextSize,
    pub end: TextSize,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TextSize(pub u32);

// intern.rs
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct Name(salsa::InternId);
```

---

### 2. `syster-parser` — Lexing & Parsing

**Stateless. No Salsa. Just text → tree.**

Produces a "green tree" (immutable, position-independent syntax tree).

```
syster-parser/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── lexer/
    │   ├── mod.rs
    │   ├── cursor.rs    # Character iteration
    │   └── token.rs     # Token kinds
    ├── parser/
    │   ├── mod.rs
    │   ├── event.rs     # Parser events (Start, Token, Finish, Error)
    │   ├── grammar/     # Grammar rules (one file per construct)
    │   │   ├── mod.rs
    │   │   ├── definitions.rs
    │   │   ├── expressions.rs
    │   │   ├── usages.rs
    │   │   └── ...
    │   └── sink.rs      # Events → GreenNode
    ├── syntax_kind.rs   # Generated enum of all node/token kinds
    └── green.rs         # GreenNode, GreenToken (rowan-style)
```

**Key insight**: Parser is a pure function `&str → (GreenNode, Vec<ParseError>)`

```rust
// lib.rs
pub fn parse(text: &str) -> Parse {
    let tokens = lexer::tokenize(text);
    let events = parser::parse(tokens);
    let (green, errors) = sink::build_tree(events, text);
    Parse { green, errors }
}

pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<ParseError>,
}
```

---

### 3. `syster-ast` — Typed Syntax Tree

Wraps green tree with typed accessors. **Still no semantics.**

```
syster-ast/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── ast.rs           # SyntaxNode, SyntaxToken wrappers
    ├── generated/       # Generated from grammar
    │   ├── mod.rs
    │   ├── nodes.rs     # PartDef, UsageDef, etc.
    │   └── tokens.rs    # Ident, Keyword, etc.
    ├── traits.rs        # HasName, HasBody, etc.
    └── visitors.rs      # Preorder/postorder traversal
```

```rust
// generated/nodes.rs (generated from grammar)
#[derive(Debug, Clone)]
pub struct PartDef {
    syntax: SyntaxNode,
}

impl PartDef {
    pub fn name(&self) -> Option<Name> {
        self.syntax.child_token(SyntaxKind::IDENT).map(Name)
    }
    
    pub fn body(&self) -> Option<Body> {
        self.syntax.child_node(SyntaxKind::BODY).map(Body)
    }
    
    pub fn specializations(&self) -> impl Iterator<Item = Specialization> {
        self.syntax.children_of_kind(SyntaxKind::SPECIALIZATION)
    }
}

// traits.rs
pub trait HasName {
    fn name(&self) -> Option<Name>;
}

impl HasName for PartDef { ... }
impl HasName for PortDef { ... }
```

---

### 4. `syster-hir` — High-level IR (Semantic Model)

**This is where Salsa lives.** All semantic analysis as queries.

```
syster-hir/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── db.rs            # Salsa database trait
    ├── input.rs         # Input queries (file text, crate graph)
    │
    ├── ids.rs           # Semantic IDs (DefId, LocalDefId)
    ├── def_map.rs       # Per-file definition map
    ├── item_tree.rs     # Lowered items (no bodies yet)
    │
    ├── resolver.rs      # Name resolution
    ├── scope.rs         # Scope tree
    ├── imports.rs       # Import resolution
    │
    ├── ty/              # Type system
    │   ├── mod.rs
    │   ├── lower.rs     # AST → Type
    │   └── infer.rs     # Type inference (if needed)
    │
    └── diagnostics.rs   # Semantic errors
```

**Key queries:**

```rust
// db.rs
#[salsa::db]
pub trait HirDb: salsa::Database {
    // === INPUTS ===
    #[salsa::input]
    fn file_text(&self, file: FileId) -> Arc<str>;
    
    #[salsa::input]
    fn source_root(&self) -> Arc<SourceRoot>;  // all files
    
    // === PARSE (cached) ===
    fn parse(&self, file: FileId) -> Arc<Parse>;
    
    // === ITEM TREE (per-file, no resolution) ===
    fn file_item_tree(&self, file: FileId) -> Arc<ItemTree>;
    
    // === DEF MAP (per-file, with resolution) ===
    fn file_def_map(&self, file: FileId) -> Arc<DefMap>;
    
    // === GLOBAL SCOPE ===
    fn crate_def_map(&self) -> Arc<CrateDefMap>;
    
    // === NAME RESOLUTION ===
    fn resolve_path(&self, from: DefId, path: &Path) -> Option<DefId>;
    
    // === TYPES ===
    fn def_type(&self, def: DefId) -> Arc<Type>;
    
    // === DIAGNOSTICS ===
    fn file_diagnostics(&self, file: FileId) -> Arc<Vec<Diagnostic>>;
}
```

**Dependency graph of queries:**

```
file_text(file)                    ← INPUT
    │
    ▼
parse(file)                        ← per-file, cheap
    │
    ▼
file_item_tree(file)               ← per-file, extracts definitions
    │
    ├─── for all files ───┐
    ▼                     ▼
file_def_map(file)    crate_def_map()   ← resolution
    │                     │
    └─────────┬───────────┘
              ▼
      resolve_path(from, path)     ← name lookup
              │
              ▼
        def_type(def)              ← type of definition
              │
              ▼
    file_diagnostics(file)         ← errors
```

---

### 5. `syster-ide` — IDE Features

Consumes HIR queries, produces IDE results. **No LSP protocol here.**

```
syster-ide/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── analysis.rs      # AnalysisHost, Analysis (snapshot)
    │
    ├── completion/
    │   ├── mod.rs
    │   └── context.rs   # Completion context
    │
    ├── goto_def.rs
    ├── find_refs.rs
    ├── hover.rs
    ├── rename.rs
    ├── diagnostics.rs   # Collect all diagnostics
    ├── folding.rs
    ├── highlights.rs
    └── inlay_hints.rs
```

```rust
// analysis.rs
pub struct AnalysisHost {
    db: RootDatabase,
}

impl AnalysisHost {
    pub fn new() -> Self { ... }
    
    /// Apply a file change
    pub fn apply_change(&mut self, change: Change) {
        self.db.apply_change(change);
    }
    
    /// Get a consistent snapshot for queries
    pub fn analysis(&self) -> Analysis {
        Analysis { db: self.db.snapshot() }
    }
}

/// Immutable snapshot, can be sent to background threads
pub struct Analysis {
    db: salsa::Snapshot<RootDatabase>,
}

impl Analysis {
    pub fn completions(&self, pos: FilePosition) -> Vec<CompletionItem> {
        completion::completions(&self.db, pos)
    }
    
    pub fn goto_definition(&self, pos: FilePosition) -> Option<Location> {
        goto_def::goto_definition(&self.db, pos)
    }
    
    pub fn hover(&self, pos: FilePosition) -> Option<HoverResult> {
        hover::hover(&self.db, pos)
    }
    
    pub fn diagnostics(&self, file: FileId) -> Vec<Diagnostic> {
        diagnostics::diagnostics(&self.db, file)
    }
}

// Types for IDE layer
pub struct FilePosition {
    pub file: FileId,
    pub offset: TextSize,
}

pub struct FileRange {
    pub file: FileId,
    pub range: TextRange,
}
```

---

### 6. `syster-lsp` — LSP Protocol

**Only handles JSON-RPC and LSP types.** Translates to/from IDE layer.

```
syster-lsp/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── main.rs          # Entry point
    │
    ├── server.rs        # Main loop, request dispatch
    ├── dispatch.rs      # Request/notification routing
    │
    ├── handlers/
    │   ├── mod.rs
    │   ├── request.rs   # Handle LSP requests
    │   └── notification.rs
    │
    ├── to_lsp.rs        # Convert IDE types → LSP types
    ├── from_lsp.rs      # Convert LSP types → IDE types
    │
    ├── vfs.rs           # Virtual file system (open docs + disk)
    └── config.rs        # Server configuration
```

```rust
// handlers/request.rs
pub fn handle_goto_definition(
    state: &ServerState,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let pos = from_lsp::file_position(&state.vfs, params.text_document_position)?;
    
    let snap = state.analysis.analysis();
    let location = snap.goto_definition(pos);
    
    Ok(location.map(|loc| to_lsp::location(&state.vfs, loc)))
}

pub fn handle_completion(
    state: &ServerState,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let pos = from_lsp::file_position(&state.vfs, params.text_document_position)?;
    
    let snap = state.analysis.analysis();
    let items = snap.completions(pos);
    
    Ok(Some(CompletionResponse::Array(
        items.into_iter().map(to_lsp::completion_item).collect()
    )))
}
```

---

## New Directory Structure

```
syster/
├── Cargo.toml              # Workspace
├── crates/
│   ├── syster-base/        # Shared primitives
│   ├── syster-parser/      # Lexer + parser
│   ├── syster-ast/         # Typed syntax tree
│   ├── syster-hir/         # Semantic model (Salsa)
│   ├── syster-ide/         # IDE features
│   └── syster-lsp/         # LSP server
├── xtask/                  # Build scripts, codegen
└── docs/
```

**Cargo.toml (workspace)**:
```toml
[workspace]
resolver = "2"
members = [
    "crates/syster-base",
    "crates/syster-parser",
    "crates/syster-ast",
    "crates/syster-hir",
    "crates/syster-ide",
    "crates/syster-lsp",
    "xtask",
]

[workspace.dependencies]
salsa = "0.18"
rowan = "0.15"
text-size = "1.1"
smol_str = "0.2"
rustc-hash = "1.1"
tracing = "0.1"
```

---

## Implementation Plan

### Phase 1: Foundation ✅ COMPLETE

**Create the skeleton crates. Get them compiling.**

- [x] `syster-base`: FileId, Span primitives
- [x] Keep existing pest parser (works fine for now)
- [x] AST types exist in `syntax/`

---

### Phase 2: Semantic Foundation ✅ MOSTLY COMPLETE

**Salsa-like caching via SymbolIndex.**

- [x] `hir/symbols.rs`: HirSymbol, SymbolKind, TypeRef
- [x] `hir/resolve.rs`: SymbolIndex with lookup methods
- [x] Bridge from legacy SymbolTable → SymbolIndex
- [ ] TODO: True Salsa queries (deferred - current approach works)

---

### Phase 3: IDE Layer ✅ MOSTLY COMPLETE

**Pure IDE functions.**

- [x] `ide/hover.rs` - hover info
- [x] `ide/goto.rs` - goto definition
- [x] `ide/references.rs` - find references
- [x] `ide/completion.rs` - completions
- [x] `ide/symbols.rs` - document/workspace symbols
- [x] `ide/document_links.rs` - clickable links
- [x] `ide/folding.rs` - folding ranges
- [x] `ide/selection.rs` - selection ranges
- [x] `ide/inlay_hints.rs` - inlay hints
- [x] `ide/semantic_tokens.rs` - semantic tokens

---

### Phase 4: AnalysisHost Pattern ✅ COMPLETE

**Replace scattered state with unified Analysis.**

- [x] Create `ide/analysis.rs` with AnalysisHost/Analysis
- [x] Move SymbolIndex + SyntaxFile storage into AnalysisHost
- [x] Implement `set_file_content()` / `remove_file()` for updates
- [ ] Update LspServer to use AnalysisHost (next step)

---

### Phase 5: LSP Handler Migration ✅ COMPLETE

**All handlers use IDE layer.**

Symbol-based handlers (✅ done):
- [x] `hover.rs`
- [x] `definition.rs`
- [x] `references.rs`
- [x] `completion.rs`
- [x] `document_symbols.rs`
- [x] `workspace_symbols.rs`
- [x] `document_links.rs`
- [x] `rename.rs`
- [x] `code_lens.rs`

AST-based handlers (✅ done):
- [x] `folding_ranges.rs` → `ide::folding_ranges()`
- [x] `selection_range.rs` → `ide::selection_ranges()`
- [x] `inlay_hints.rs` → `ide::inlay_hints()`
- [x] `semantic_tokens.rs` → `ide::semantic_tokens()`

---

### Phase 6: Migrate LspServer to AnalysisHost 🔄 NEXT

**Replace scattered state with AnalysisHost + Fix Name Resolution.**

The current resolution is broken because `hover.rs` does flat global lookups
(`lookup_simple().first()`) instead of scope-aware resolution using imports.
The `Resolver` type exists but isn't being used with the file's imports.

#### Problem Analysis

```
Current (broken):
hover.rs → index.lookup_simple("Real").first()  // Returns ANY "Real" symbol

Target (correct - like rust-analyzer):
hover.rs → resolver.resolve_type("Real")        // Uses imports in scope
           ↓
           Resolver::resolve() checks:
           1. Current scope (SimpleVehicleModel::VehicleAnalysis::ComputeBSFC)
           2. Imports visible in scope (transitively)
           3. Global scope
```

#### Files to Change

**1. `hir/symbols.rs` — Capture import visibility info**

Current `extract_from_import()` creates symbols but doesn't track:
- Whether the import is `public` (re-exported to children)
- The containing scope where the import appears

Changes:
```rust
// Add to HirSymbol or create new ImportInfo structure
pub struct HirSymbol {
    // ... existing fields ...
    pub is_public: bool,  // NEW: For imports, whether re-exported
}

// Update extract_from_import() to set is_public from import.is_public
```

**2. `hir/resolve.rs` — Add scope-aware resolver builder**

Current `Resolver` exists but must be manually configured with imports.
Add a method to build a resolver from a scope:

```rust
impl SymbolIndex {
    /// Build a resolver with all imports visible at the given scope.
    ///
    /// Collects imports from:
    /// 1. The scope itself (e.g., "Pkg::VehicleAnalysis::ComputeBSFC")
    /// 2. Parent scopes (e.g., "Pkg::VehicleAnalysis", "Pkg")
    /// 3. Transitively follows `public import` re-exports
    pub fn resolver_for_scope(&self, scope: &str) -> Resolver<'_> {
        let mut resolver = Resolver::new(self).with_scope(scope);
        
        // Collect imports at each scope level
        let mut current = scope.to_string();
        loop {
            // Find imports declared in this scope
            for symbol in self.all_symbols() {
                if symbol.kind == SymbolKind::Import 
                    && symbol.qualified_name.starts_with(&current)
                    && is_direct_child(&symbol.qualified_name, &current) 
                {
                    // Extract the imported namespace from "Pkg::import:ISQ::*"
                    if let Some(import_path) = extract_import_path(&symbol.name) {
                        resolver = resolver.with_import(import_path);
                        
                        // If public import, follow transitive exports
                        if symbol.is_public {
                            self.collect_transitive_imports(&mut resolver, &import_path);
                        }
                    }
                }
            }
            
            // Move up to parent scope
            if let Some(idx) = current.rfind("::") {
                current = current[..idx].to_string();
            } else {
                break;
            }
        }
        
        resolver
    }
    
    /// Follow public re-exports transitively.
    fn collect_transitive_imports(&self, resolver: &mut Resolver<'_>, namespace: &str) {
        // Look for public imports inside the target namespace
        // e.g., "Definitions" has "public import AttributeDefinitions::*"
        // ...
    }
}
```

**3. `ide/hover.rs` — Use scope-aware resolution**

Current code does flat lookup. Change to use resolver:

```rust
// BEFORE (broken):
let target_symbol = index.lookup_definition(&target_name)
    .or_else(|| index.lookup_qualified(&target_name))
    .or_else(|| index.lookup_simple(&target_name).first());

// AFTER (correct):
// Find the containing scope for this type_ref
let containing_scope = find_containing_scope(index, file, line);
let resolver = index.resolver_for_scope(&containing_scope);

let target_symbol = match resolver.resolve_type(&target_name) {
    ResolveResult::Found(sym) => Some(sym),
    ResolveResult::Ambiguous(syms) => syms.first().cloned(),
    ResolveResult::NotFound => None,
};
```

**4. `ide/analysis.rs` — Expose resolver through Analysis**

Add helper method to get a resolver for a position:

```rust
impl Analysis<'_> {
    /// Get a resolver configured for the scope at the given position.
    pub fn resolver_at(&self, file: FileId, line: u32, col: u32) -> Resolver<'_> {
        let scope = self.find_scope_at_position(file, line, col);
        self.symbol_index.resolver_for_scope(&scope)
    }
    
    fn find_scope_at_position(&self, file: FileId, line: u32, col: u32) -> String {
        // Find the innermost symbol containing this position
        // Return its qualified_name as the scope
    }
}
```

#### Implementation Order

1. **Update `HirSymbol`** — Add `is_public` field
2. **Update `extract_from_import()`** — Set `is_public` from AST
3. **Add `resolver_for_scope()`** — Core resolution logic
4. **Add `collect_transitive_imports()`** — Follow `public import` chains
5. **Update `hover.rs`** — Use resolver instead of flat lookup
6. **Add `resolver_at()` to Analysis** — Convenient access point
7. **Update `goto.rs`, `references.rs`** — Same pattern as hover

#### Testing Strategy

The failing tests demonstrate the problem:
- `test_real_resolves_in_calc_def` — `Real` via transitive `public import` chain
- `test_real_resolves_in_parameter` — Same issue

After implementation, these should pass because the resolver will:
1. See `public import Definitions::*` in `SimpleVehicleModel`
2. Follow to `Definitions` → see `public import AttributeDefinitions::*`
3. Follow to `AttributeDefinitions` → see `public import ScalarValues::*`
4. Resolve `Real` in `ScalarValues::Real`

---

- [ ] Add `is_public` field to `HirSymbol` (`hir/symbols.rs`)
- [ ] Update `extract_from_import()` to capture `is_public` (`hir/symbols.rs`)
- [ ] Add `resolver_for_scope()` method to `SymbolIndex` (`hir/resolve.rs`)
- [ ] Add `collect_transitive_imports()` for public re-exports (`hir/resolve.rs`)
- [ ] Update `hover.rs` to use scope-aware resolution (`ide/hover.rs`)
- [ ] Add `resolver_at()` helper to `Analysis` (`ide/analysis.rs`)
- [ ] Update `goto.rs` to use scope-aware resolution (`ide/goto.rs`)
- [ ] Replace `workspace` + `symbol_index` + `file_id_map` with `AnalysisHost` (LspServer)
- [ ] Update handlers to use `analysis.analysis()` snapshot
- [ ] Remove manual `rebuild_symbol_index()` calls

---

### Phase 7: Remove Legacy Code ⏳ PENDING

**Clean up old architecture.**

- [ ] Remove `Workspace` from LspServer (will be inside AnalysisHost)
- [ ] Remove bridge converter (used internally by AnalysisHost)
- [ ] Clean up unused semantic layer code

---

## Name Resolution Architecture (January 2026 Design)

### Problem Statement

The current resolution is **ad-hoc and broken** for complex scenarios:

```rust
// Current broken approach in resolve.rs:
pub fn resolve(&self, name: &str) -> ResolveResult {
    // 1. Check if qualified name - early return if not found!
    if name.contains("::") {
        if let Some(symbol) = self.index.lookup_qualified(name) {
            return ResolveResult::Found(symbol.clone());
        }
        return ResolveResult::NotFound;  // ← BROKEN: doesn't follow aliases/re-exports!
    }
    // ... rest of resolution
}
```

This fails for:
- `ISQ::TorqueValue` where ISQ is a package with `public import ISQSpaceTime::*`
- Alias targets like `alias Torque for ISQ::TorqueValue`
- Transitive re-exports through multiple levels

### How rust-analyzer Does It

1. **DefMap per Module** - A `DefMap` captures:
   - Symbols defined directly in the module
   - Symbols imported (and their source)
   - Symbols publicly re-exported (visible to children/importers)
   - Visibility rules (public vs private)

2. **Name Resolution is Separate** - Resolution doesn't happen during symbol extraction:
   - HIR extraction captures raw names/references with spans
   - A separate pass builds `DefMap` with resolved imports
   - Query-time resolution uses pre-computed `DefMap`

3. **Transitive Re-exports** - When `package ISQ { public import ISQSpaceTime::* }`:
   - ISQ's `DefMap` includes everything public from ISQSpaceTime
   - Looking up `ISQ::TorqueValue` finds `ISQSpaceTime::TorqueValue`

### Proposed Architecture

#### Data Structures

```rust
/// Per-scope visibility map (built once at index time, used at query time)
pub struct ScopeVisibility {
    /// The scope this visibility applies to (e.g., "ISQ", "Automotive::Torque")
    scope: Arc<str>,
    
    /// Symbols defined directly in this scope
    /// SimpleName → QualifiedName
    direct_defs: HashMap<Arc<str>, Arc<str>>,
    
    /// Symbols visible via imports (includes transitive public re-exports)
    /// SimpleName → QualifiedName (the resolved target)
    imports: HashMap<Arc<str>, Arc<str>>,
    
    /// Public re-exports from this scope (for transitive import resolution)
    /// Namespaces that are publicly re-exported
    public_reexports: Vec<Arc<str>>,
}

/// Extended SymbolIndex with pre-computed visibility
pub struct SymbolIndex {
    // ... existing fields ...
    
    /// Pre-computed visibility map for each scope
    visibility_map: HashMap<Arc<str>, ScopeVisibility>,
}
```

#### Build Phase (During Index Construction)

```rust
impl SymbolIndex {
    /// Build visibility maps for all scopes after symbol extraction
    fn build_visibility_maps(&mut self) {
        // 1. Identify all scopes (packages, definitions with bodies)
        let scopes = self.collect_all_scopes();
        
        // 2. For each scope, collect direct definitions
        for scope in &scopes {
            let vis = ScopeVisibility::new(scope);
            vis.direct_defs = self.collect_direct_defs(scope);
            self.visibility_map.insert(scope.clone(), vis);
        }
        
        // 3. Process imports (may need multiple passes for transitive)
        let mut changed = true;
        while changed {
            changed = false;
            for scope in &scopes {
                if self.process_imports_for_scope(scope) {
                    changed = true;
                }
            }
        }
    }
    
    /// Process imports for a scope, following public re-exports
    fn process_imports_for_scope(&mut self, scope: &str) -> bool {
        let mut new_imports = HashMap::new();
        
        // Find import symbols in this scope
        for symbol in self.symbols_in_scope(scope) {
            if symbol.kind == SymbolKind::Import {
                let import_target = extract_import_target(&symbol.name);
                
                // Resolve the import target
                if let Some(target_scope) = self.visibility_map.get(import_target) {
                    // Add all direct defs from target
                    for (name, qname) in &target_scope.direct_defs {
                        new_imports.insert(name.clone(), qname.clone());
                    }
                    
                    // If public import, also add to re-exports
                    if symbol.is_public {
                        // ... track for child scopes
                    }
                }
            }
        }
        
        // Merge new imports into visibility map
        let vis = self.visibility_map.get_mut(scope).unwrap();
        let had_changes = !new_imports.is_empty();
        vis.imports.extend(new_imports);
        had_changes
    }
}
```

#### Query Phase (Resolution)

```rust
impl Resolver<'_> {
    /// Resolve a name using pre-computed visibility maps
    pub fn resolve(&self, name: &str) -> ResolveResult {
        // 1. Try as fully qualified name
        if let Some(symbol) = self.index.lookup_qualified(name) {
            return ResolveResult::Found(symbol.clone());
        }
        
        // 2. For qualified names like "ISQ::TorqueValue"
        if name.contains("::") {
            return self.resolve_qualified_path(name);
        }
        
        // 3. Check current scope's visibility map
        if let Some(vis) = self.index.visibility_map.get(&self.current_scope) {
            // Check direct definitions first
            if let Some(qname) = vis.direct_defs.get(name) {
                if let Some(sym) = self.index.lookup_qualified(qname) {
                    return ResolveResult::Found(sym.clone());
                }
            }
            
            // Check imports
            if let Some(qname) = vis.imports.get(name) {
                if let Some(sym) = self.index.lookup_qualified(qname) {
                    return ResolveResult::Found(sym.clone());
                }
            }
        }
        
        // 4. Walk up scope chain
        if let Some(parent_scope) = parent_of(&self.current_scope) {
            return self.with_scope(parent_scope).resolve(name);
        }
        
        ResolveResult::NotFound
    }
    
    /// Resolve "ISQ::TorqueValue" through visibility maps
    fn resolve_qualified_path(&self, path: &str) -> ResolveResult {
        let (first, rest) = split_first_segment(path);
        
        // Resolve first segment (might be in current scope, imported, or global)
        let first_result = self.resolve(first);
        
        match first_result {
            ResolveResult::Found(first_sym) => {
                // If it's a scope (package/namespace), look in its visibility map
                let target_scope = first_sym.qualified_name.as_ref();
                
                if let Some(vis) = self.index.visibility_map.get(target_scope) {
                    // Look for 'rest' in that scope
                    if let Some(qname) = vis.direct_defs.get(rest)
                        .or_else(|| vis.imports.get(rest)) 
                    {
                        if let Some(sym) = self.index.lookup_qualified(qname) {
                            return ResolveResult::Found(sym.clone());
                        }
                    }
                }
                
                // Try direct qualified lookup (might be nested)
                let full_path = format!("{}::{}", target_scope, rest);
                if let Some(sym) = self.index.lookup_qualified(&full_path) {
                    return ResolveResult::Found(sym.clone());
                }
            }
            _ => {}
        }
        
        ResolveResult::NotFound
    }
}
```

### Implementation Phases

#### Phase A: Add ScopeVisibility Structure
- [ ] Create `ScopeVisibility` struct in `hir/resolve.rs`
- [ ] Add `visibility_map` field to `SymbolIndex`
- [ ] Add `collect_all_scopes()` helper

#### Phase B: Build Visibility During Index Construction
- [ ] Add `build_visibility_maps()` method
- [ ] Call it at end of `SymbolIndex::add_file()` or in `rebuild_index()`
- [ ] Handle basic direct definitions

#### Phase C: Process Imports
- [ ] Implement `process_imports_for_scope()`
- [ ] Handle wildcard imports (`ISQ::*`)
- [ ] Handle specific imports (`ISQ::TorqueValue`)
- [ ] Track public re-exports

#### Phase D: Transitive Re-exports
- [ ] Iterate until fixed-point for transitive public imports
- [ ] Handle: `ISQ { public import ISQSpaceTime::* }` means ISQ re-exports ISQSpaceTime's publics

#### Phase E: Update Resolver
- [ ] Rewrite `resolve()` to use visibility maps
- [ ] Rewrite `resolve_qualified_path()` for `ISQ::TorqueValue` case
- [ ] Remove ad-hoc alias chasing

#### Phase F: Update IDE Layer
- [ ] Update `hover.rs` to use new resolver
- [ ] Update `goto.rs` to use new resolver
- [ ] Update `references.rs` to use new resolver

### Testing Strategy

1. **Unit tests for ScopeVisibility**
   - Direct definitions are found
   - Imports are resolved correctly
   - Public re-exports propagate

2. **Integration tests**
   - `ISQ::TorqueValue` resolves via `public import ISQSpaceTime::*`
   - Alias targets resolve correctly
   - Nested scopes see parent imports

3. **Regression tests**
   - All existing passing tests continue to pass
   - Previously failing stdlib resolution tests now pass

### Success Metrics

- [ ] `test_hover_on_qualified_alias_target` passes
- [ ] `test_hover_isq_massvalue_extension_stdlib` passes
- [ ] `test_hover_torque_alias_with_stdlib` passes
- [ ] Resolution is O(1) lookup instead of O(n) iteration
- [ ] No runtime allocation during resolution (uses pre-built maps)

---

## Key Design Decisions

### 1. Ditch pest, write hand-written parser

**Why**: 
- Better error recovery
- Incremental-friendly (can re-parse just changed region)
- Full control over tree structure
- Matches rust-analyzer approach

**Status**: DEFERRED - pest parser works fine for MVP. Revisit if perf issues.

**Parser style**: Recursive descent with explicit event emission

```rust
fn definition(p: &mut Parser) {
    let m = p.start();
    
    match p.current() {
        T![part] => part_def(p),
        T![port] => port_def(p),
        T![action] => action_def(p),
        _ => {
            p.error("expected definition");
            p.bump_any(); // error recovery
        }
    }
    
    m.complete(p, SyntaxKind::DEFINITION);
}
```

### 2. Use Rowan for syntax trees

**Why**:
- Immutable, cheap to clone
- Green/Red tree pattern (position-independent storage)
- Battle-tested in rust-analyzer
- Supports incremental re-parsing

### 3. Salsa only in `syster-hir`

**Why**:
- Parser stays simple and fast
- AST is just a view over syntax
- Only semantic queries need caching
- Clear boundary for incremental computation

### 4. Separate IDE from LSP

**Why**:
- IDE features are testable without LSP
- Can support multiple frontends (LSP, CLI, web)
- LSP layer is just protocol translation
- Easier to debug issues

### 5. Item Tree as intermediate representation

**Why**:
- Extracts "shape" of definitions without full resolution
- Per-file, independent of other files
- Foundation for incremental def maps
- Avoids re-parsing when only types change

```rust
// item_tree.rs
pub struct ItemTree {
    pub items: Vec<Item>,
}

pub enum Item {
    PartDef(PartDef),
    PortDef(PortDef),
    ActionDef(ActionDef),
    // ...
}

pub struct PartDef {
    pub name: Name,
    pub visibility: Visibility,
    pub specializes: Vec<TypeRef>,
    pub body: Option<ItemTreeId>,  // nested items
}
```

---

## What We're Keeping vs Removing

### Keeping (still used during transition):

| Component | Status | Notes |
|-----------|--------|-------|
| `pest` grammar | ✅ Keep | Works fine, replace later if needed |
| `SymbolTable` | ✅ Keep (internal) | Bridge converts to SymbolIndex |
| `ReferenceIndex` | ✅ Keep (internal) | Populates TypeRef data |
| `Workspace` | ⚠️ Transitional | Will remove after AnalysisHost |
| `SysmlAdapter/KermlAdapter` | ✅ Keep | Still parse SysML/KerML files |
| `extract_*` functions | ✅ Keep | Reused by IDE layer |

### Removing (after full migration):

| Component | Replacement |
|-----------|-------------|
| `Workspace` in LspServer | `AnalysisHost` |
| `resolver()` method | `ide::goto_definition()` |
| `symbol_table()` access from LSP | `SymbolIndex` via Analysis |
| Manual `rebuild_symbol_index()` | `analysis.apply_change()` |
| Direct AST access from LSP | IDE functions |

---

## Actual Timeline (Revised)

| Phase | Status | Notes |
|-------|--------|-------|
| Foundation | ✅ Done | FileId, Span, base types |
| HIR Layer | ✅ Done | SymbolIndex, HirSymbol |
| IDE Layer (symbols) | ✅ Done | hover, goto, refs, completion |
| IDE Layer (AST) | 🔄 Next | folding, selection, hints, tokens |
| AnalysisHost | ⏳ Pending | Unified state management |
| LSP Migration | 🔄 75% | 4 handlers remaining |
| Legacy Removal | ⏳ Pending | After full migration |

**Estimated remaining: 2-3 weeks**

---

## Success Criteria

1. **Correctness**: All existing test cases pass
2. **Incrementality**: Edit single file → only that file re-analyzed
3. **Performance**: < 50ms for any IDE query on warm cache
4. **Maintainability**: Each crate < 5k LOC, clear responsibilities
5. **Extensibility**: Adding new definition type touches 3-4 files max

---

## Getting Started

```bash
# Create workspace
mkdir -p syster-new/crates
cd syster-new

# Initialize workspace Cargo.toml
cat > Cargo.toml << 'EOF'
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.dependencies]
salsa = "0.18"
rowan = "0.15"
text-size = "1.1"
EOF

# Create first crate
cargo new --lib crates/syster-base
cargo new --lib crates/syster-parser

# Start coding
```

---

## Reference

- [rust-analyzer architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md)
- [Salsa book](https://salsa-rs.github.io/salsa/)
- [Rowan crate](https://github.com/rust-analyzer/rowan)
- [Simple Rust parser example](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
