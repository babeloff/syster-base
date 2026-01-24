# Changelog

All notable changes to syster-base will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1-alpha] - 2026-01-24

### Added

- **Relationships in HIR**: Symbols now track their relationships to other symbols
  - `HirRelationship` — Represents a relationship between symbols with kind and target
  - `RelationshipKind` — Enum covering Specializes, TypedBy, Subsets, Redefines, References, Satisfies, Performs, Exhibits, Includes, Asserts, Verifies
  - `HirSymbol.relationships` — Vector of relationships extracted during symbol extraction

- **Type Information API** (`ide/type_info.rs`):
  - `type_info_at` — Retrieve type information at a specific cursor position
  - `goto_type_definition` — Navigate directly from usages to their type definitions
  - `TypeInfo` — Struct containing type name, definition location, and span info

- **Resolved Relationships in Hover**:
  - `ResolvedRelationship` — Pre-resolved relationship with target file/line info for clickable links
  - Hover results now include resolved relationships for LSP to render as navigable links

### Changed

- **Hover Result**: Now includes `relationships: Vec<ResolvedRelationship>` with pre-resolved target locations
- **Symbol Extraction**: Extracts relationships from specialization, typing, subsetting, and other relationship constructs

## [0.2.0-alpha] - 2026-01-23

### 🚀 Major Rewrite — Salsa-based Incremental Architecture

This release represents a complete architectural rewrite, moving from an eager/imperative model to a query-based incremental computation system using [Salsa](https://github.com/salsa-rs/salsa).

### Added

- **Salsa Integration**: Full migration to Salsa for incremental, memoized queries
  - `RootDatabase` — The root Salsa database holding all query storage
  - `FileText` — Input query for raw source text
  - `SourceRootInput` — Input query for workspace file configuration
  - `parse_file` — Tracked query that parses source into AST
  - `file_symbols` — Query to extract HIR symbols from parsed AST
  - `file_symbols_from_text` — Combined parsing + symbol extraction query

- **Foundation Types** (`base` module):
  - `FileId` — Lightweight 4-byte interned file identifier (replaces `PathBuf` for O(1) comparisons)
  - `Name` — Interned identifier handle for O(1) string comparisons
  - `Interner` — Thread-safe string interner using `parking_lot` and `smol_str`
  - `TextRange`, `TextSize` — Source position types (re-exported from `text-size`)
  - `LineCol`, `LineIndex` — Line/column conversion utilities

- **Semantic IDs**:
  - `DefId` — Globally unique definition identifier (FileId + LocalDefId)
  - `LocalDefId` — File-local definition ID for efficient per-file invalidation

- **Input Management**:
  - `SourceRoot` — Workspace file registry with efficient insertion/removal

- **Anonymous scope naming**: Anonymous usages get unique qualified names using `<prefix#counter@Lline>` format
  - Relationship prefixes: `:>`, `:`, `:>:`, `:>>`, `about:`, `perform:`, `satisfy:`, `exhibit:`, `include:`, `assert:`, `verify:`, `ref:`, `meta:`, `crosses:`

- **Invocation expression reference extraction**: Function invocations like `EngineEvaluation_6cyl(...)` now extract the function name as a reference

- **Import link resolution for same-file packages**: Document links for imports use scope-aware `Resolver`

- **Implicit Supertypes**: All definitions now automatically inherit from their SysML kernel metaclass
  - `part def` → `Parts::Part`
  - `item def` → `Items::Item`
  - `action def` → `Actions::Action`
  - `state def` → `States::StateAction`
  - `constraint def` → `Constraints::ConstraintCheck`
  - `requirement def` → `Requirements::RequirementCheck`
  - `calc def` → `Calculations::Calculation`
  - `port def` → `Ports::Port`
  - `connection def` → `Connections::Connection`
  - `interface def` → `Interfaces::Interface`
  - `allocation def` → `Allocations::Allocation`
  - `use case def` → `UseCases::UseCase`
  - `analysis case def` → `AnalysisCases::AnalysisCase`
  - `attribute def` → `Attributes::AttributeValue`
  - Usage kinds: `flow` → `Flows::Message`, `connection` → `Connections::Connection`, etc.

- **Semantic Diagnostics System** (`diagnostics` module): Brand new semantic error reporting infrastructure
  - `Diagnostic` — Rich diagnostic type with file, span, severity, code, message, and related info
  - `Severity` — Error, Warning, Info, Hint levels with LSP conversion
  - `RelatedInfo` — Additional context linking to other source locations
  - `DiagnosticCollector` — Accumulator for diagnostics during analysis
  - `SemanticChecker` — Full semantic analysis engine that validates:
    - Undefined references (E0001)
    - Ambiguous references (E0002)
    - Type mismatches (E0003)
    - Duplicate definitions (E0004)
    - Missing required elements (E0005)
    - Invalid specialization (E0006)
    - Circular dependencies (E0007)
    - Unused symbols (W0001)
    - Deprecated usage (W0002)
    - Naming convention violations (W0003)
  - `check_file()` — Per-file semantic validation with duplicate detection
  - Deduplication in `finish()` — Filters duplicate diagnostics (same file, line, col, message)

### Changed

- **Complete HIR rewrite**: All semantic analysis now flows through Salsa queries
  - Automatic memoization — queries only re-run when inputs change
  - Automatic invalidation — change a file, only affected queries recompute
  - Parallel-safe — Salsa's design enables concurrent query execution

- **Memory efficiency**:
  - `FileId` (4 bytes) replaces `PathBuf` (~24+ bytes)
  - `Name` (4 bytes) for interned identifiers
  - `Arc<str>` for shared strings with reference counting

- `ExtractionContext` now includes `anon_counter: u32` and `next_anon_scope()` method

### Removed

- **Old `semantic` module**: Deleted the entire eager/imperative semantic analysis system
  - Removed `semantic/symbol_table/` — replaced by `hir::SymbolIndex`
  - Removed `semantic/workspace/` — replaced by Salsa database
  - Removed `semantic/adapters/` — replaced by `hir::symbols::extract_symbols_unified`
  - Removed `semantic/resolver/` — replaced by `hir::resolve::Resolver`
  - Removed `semantic/graphs/` — reference tracking now built into `SymbolIndex`

### Performance

- **Incremental parsing**: Only re-parse files that actually changed
- **Memoized symbol extraction**: Symbol extraction cached per-file
- **O(1) file/name comparisons**: Interned identifiers enable constant-time equality checks
- **Reduced memory pressure**: Shared string storage via interning

## [0.1.12-alpha] - 2025-01-30

### Added

- Initial feature chain resolution for SysML models
- Basic semantic analysis and name resolution
- HIR symbol extraction with type references
