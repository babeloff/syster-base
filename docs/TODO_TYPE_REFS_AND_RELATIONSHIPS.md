# TODO: Type References Feature & Relationships in Hover

## Overview

This document outlines the plan to:
1. ✅ Extract type references into a dedicated IDE feature
2. ✅ Remove type references from hover 
3. ✅ Add relationship information to hover instead

## Current State (COMPLETED)

### ✅ Phase 1: Add Relationships to Hover

**Completed!** The following changes were made:

1. **Added RelationshipKind enum** in `hir/symbols.rs`:
   - `Specializes`, `TypedBy`, `Redefines`, `Subsets`, `References`
   - Domain-specific: `Satisfies`, `Performs`, `Exhibits`, `Includes`, `Asserts`, `Verifies`
   - With `from_normalized()` and `display()` methods

2. **Added HirRelationship struct** in `hir/symbols.rs`:
   - `kind: RelationshipKind`
   - `target: Arc<str>` 
   - `resolved_target: Option<Arc<str>>`

3. **Extended HirSymbol** with `relationships: Vec<HirRelationship>` field

4. **Updated symbol extraction** to populate relationships from normalized layer

5. **Updated hover content** in `ide/hover.rs`:
   - Relationships displayed grouped by kind
   - Shows brief documentation for targets when available
   - Displays in logical order: Specializes → TypedBy → Subsets → Redefines → References → domain-specific

---

## ✅ Phase 2: Extract Type References to Dedicated Feature

**Completed!** The following changes were made:

### 2.1 Created `ide/type_info.rs` module

New module with:
- `TypeInfo` struct with `target_name`, `type_ref`, `resolved_symbol`, `container`
- `type_info_at()` - main entry point for getting type info at a position
- `find_type_ref_at_position()` - moved from hover.rs
- `resolve_type_ref()` - resolution logic extracted for reuse
- `type_refs_in_symbol()` - helper to get all type refs in a symbol
- Unit tests for type info queries

### 2.2 Updated `ide/mod.rs`

Exports:
- `TypeInfo`
- `type_info_at`
- `find_type_ref_at_position`
- `resolve_type_ref`

### 2.3 Simplified `ide/hover.rs`

- Removed duplicated `find_type_ref_at_position()` function (now in type_info.rs)
- Removed debug logging (was only for specific symbol debugging)
- Uses `type_info::find_type_ref_at_position` and `type_info::resolve_type_ref`
- Cleaner separation: hover.rs focuses on content building

### 2.4 LSP Integration

**Completed!** Added custom LSP request `syster/typeInfo`:

1. **Created `server/type_info.rs`** in syster-lsp:
   - `TypeInfoRequest` - LSP request type
   - `TypeInfoParams` - request parameters (uri, position)
   - `TypeInfoResult` - response with target name, resolved name, kind, doc, container, ref_kind, span
   - `LspServer::get_type_info()` - handler implementation

2. **Registered request** in `main.rs`:
   ```rust
   router.request::<TypeInfoRequest, _>(|state, params| { ... });
   ```

3. **Backwards Compatibility**:
   - Added `core` module re-exports in syster-base for legacy `syster::core::*` imports
   - This allows syster-lsp to continue working while migrating to new paths

---

## Phase 3: Optional Enhancements (TODO)

### 3.1 Type hierarchy view
- Show full inheritance chain in hover
- "Implements: X, Y, Z" for interface conformance

### 3.2 Reverse relationships in hover
- Use existing `ide/references.rs` `find_references()` to show "Used by: A, B, C"
- No new index needed - leverage existing reference finding

### 3.3 Inlay hints for relationships
- Show relationship icons inline: `part engine ⊳ Engine`

---

## Execution Checklist

### ✅ Phase 1: Relationships in Hover
- [x] Add `HirRelationship` and `RelationshipKind` to `hir/symbols.rs`
- [x] Update symbol extraction to populate relationships
- [x] Keep `supertypes` for backwards compat
- [x] Update `build_hover_content()` to show all relationship kinds
- [x] Add tests for relationship display in hover

### ✅ Phase 2: Type References Feature  
- [x] Create `ide/type_info.rs` module
- [x] Move `find_type_ref_at_position()` to new module
- [x] Create `type_info_at()` function
- [x] Extract `resolve_type_ref()` for reuse
- [x] Simplify hover.rs to use type_info module
- [x] Add tests for type_info module

### Phase 3: Cleanup (Optional)
- [ ] Add LSP custom request for type info
- [ ] Update documentation
- [ ] Run full test suite

---

## Benefits

1. **Cleaner separation of concerns**: Hover shows symbol info, type_info shows type annotations
2. **Richer relationship display**: Users see all relationships (specializes, satisfies, performs, etc.)
3. **Better navigation**: Type references can be a dedicated feature with more options
4. **Simpler hover code**: Remove complex type ref detection logic

---

*Created: January 23, 2026*
