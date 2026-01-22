# TODO: Proper Feature Chain Resolution

## Problem Statement

Our current approach to feature chains (`obj.field.method`) is fundamentally flawed:

1. **Parser emits flat refs, resolution guesses structure from spans**
   - We detect chains by checking if spans are adjacent (end_col + 1 == start_col)
   - This is fragile and causes false groupings (e.g., 12-element chains from expressions)
   
2. **Resolution happens too late and lacks type information**
   - We try to resolve `obj.field` by resolving `obj`, then looking up `field` in its type
   - But resolution happens in a single pass without proper type propagation

## How rust-analyzer Does It

rust-analyzer has explicit AST structure:

```rust
FieldExpr {
    base: FieldExpr {
        base: PathExpr("foo"),
        field: "bar"
    },
    field: "baz"
}
```

Resolution walks the tree:
1. Name resolution: Find what `foo` refers to
2. Type inference: Compute type of `foo` 
3. Member lookup: Find `bar` in that type
4. Repeat for nested accesses

## Current Grammar Analysis

### SysML Grammar (sysml.pest)

```pest
# Already has explicit chain structure!
owned_feature_chain = @{ (identifier | quoted_name) ~ ("." ~ (identifier | quoted_name))+ }

# Used in many places:
# - feature_chain_member (line 530)
# - primary_expression (line 482) - "." ~ feature_chain_member
# - flow_end_member (line 1838-1840)
# - succession rules (lines 2344-2359)
```

### KerML Grammar (kerml.pest)

```pest
owned_feature_chain = { identifier ~ ("." ~ identifier)+ }
feature_chain_member = { feature_reference | owned_feature_chain }
feature_chain_expression = { operator_expression }
qualified_reference_chain = { ... }  # Line 1220
```

### Parser Handler (parsers.rs)

Current handler at line 170 already extracts chain parts separately:
```rust
Rule::owned_feature_chain => {
    // For feature chains like `pwrCmd.pwrLevel`, emit each part as a separate reference
    // with chain context for proper resolution.
    let chain_parts: Vec<String> = raw.split('.').map(...).collect();
    // ... emits each part with chain_context
}
```

## The Real Problem

The grammar and parser DO have `owned_feature_chain` as structured data. The problem is:

1. **AST types don't preserve chain structure** - We extract to flat `ExtractedRef` with `chain_context`
2. **Normalized layer loses chain info** - `NormalizedRelationship` has just `target: &str`
3. **HIR `TypeRef` loses chain info** - Just has `target: Arc<str>`, no chain structure
4. **Resolution reconstructs chains from spans** - Fragile workaround

---

## Implementation Plan

### Phase 1: Preserve Chain Structure in AST Types

- [ ] **1.1** Add `FeatureChain` struct to AST types:
  ```rust
  pub struct FeatureChain {
      pub parts: Vec<FeatureChainPart>,
      pub span: Option<Span>,
  }
  
  pub struct FeatureChainPart {
      pub name: String,
      pub span: Option<Span>,
  }
  ```

- [ ] **1.2** Update `ExtractedRef` to handle chains:
  ```rust
  pub enum ExtractedRef {
      Simple { name: String, span: Option<Span> },
      Chain(FeatureChain),
  }
  ```

- [ ] **1.3** Update `collect_refs_recursive` (parsers.rs line 170) to emit `ExtractedRef::Chain` for `owned_feature_chain`

### Phase 2: Preserve Chain Structure in Normalized Layer

- [ ] **2.1** Add chain variant to `NormalizedRelationship`:
  ```rust
  pub struct NormalizedRelationship<'a> {
      pub kind: NormalizedRelKind,
      pub target: RelTarget<'a>,  // Change from &str
      pub span: Option<Span>,
  }
  
  pub enum RelTarget<'a> {
      Simple(&'a str),
      Chain(Vec<(&'a str, Option<Span>)>),
  }
  ```

- [ ] **2.2** Update SysML → Normalized conversion to preserve chains

- [ ] **2.3** Update KerML → Normalized conversion to preserve chains

### Phase 3: Preserve Chain Structure in HIR

- [ ] **3.1** Add `TypeRefChain` to HIR symbols:
  ```rust
  pub enum TypeRefKind {
      Simple(TypeRef),
      Chain(TypeRefChain),
  }
  
  pub struct TypeRefChain {
      pub parts: Vec<TypeRef>,  // Each part has its own span
  }
  ```

- [x] **3.2** Update `extract_type_refs_from_normalized` to emit chains

- [x] **3.3** Update `HirSymbol.type_refs` to use `TypeRefKind`

### Phase 4: Proper Two-Phase Resolution

- [x] **4.1** Remove span-adjacency chain detection (`detect_chains_from_spans`) - removed from resolve.rs

- [x] **4.2** Implement proper chain resolution in `resolve_type_ref` - now uses `resolve_feature_chain_member`

- [x] **4.3** Update LSP hover to use `TypeRefKind.as_refs()` for iteration

- [x] **4.4** Update LSP code_lens to use `TypeRefKind.as_refs()` for iteration

- [x] **4.5** Update LSP position.rs to use `TypeRefKind.part_at()` for lookup

- [x] **4.6** Update LSP test_helpers.rs to use `TypeRefKind.as_refs()` for iteration

- [ ] **4.3** Update hover to use chain resolution

- [ ] **4.4** Update goto-definition to use chain resolution

### Phase 5: Testing & Cleanup

- [ ] **5.1** Add unit tests for chain extraction at each layer
- [ ] **5.2** Add integration tests for chain resolution
- [ ] **5.3** Verify vehicle example test passes (317 failures → 0)
- [ ] **5.4** Remove deprecated span-based chain detection code
- [ ] **5.5** Clean up debug logging

---

## Key Files to Modify

| File | Changes |
|------|---------|
| `src/syntax/sysml/ast/types.rs` | Add `FeatureChain`, `FeatureChainPart` |
| `src/syntax/sysml/ast/parsers.rs` | Update chain extraction (line 170) |
| `src/syntax/kerml/ast/types.rs` | Add `FeatureChain` (if not shared) |
| `src/syntax/normalized.rs` | Add `RelTarget::Chain` variant |
| `src/hir/symbols.rs` | Add `TypeRefKind`, `TypeRefChain` |
| `src/hir/resolve.rs` | Implement proper chain resolution, remove span detection |
| `src/ide/hover.rs` | Update to use chain resolution |
| `src/ide/goto_definition.rs` | Update to use chain resolution |

---

## Dependencies & Order

```
Phase 1 (AST) → Phase 2 (Normalized) → Phase 3 (HIR) → Phase 4 (Resolution) → Phase 5 (Testing)
```

Each phase can be tested independently before moving to the next.

---

## Estimated Effort

- Phase 1: 2-3 hours
- Phase 2: 2-3 hours
- Phase 3: 2-3 hours
- Phase 4: 4-6 hours (most complex)
- Phase 5: 2-3 hours

**Total: ~15-20 hours**

---

## Notes

- The grammar already parses `owned_feature_chain` as a unit - we just lose the structure in conversion
- KerML has similar patterns (`feature_chain_expression`, `qualified_reference_chain`)
- Consider unifying SysML and KerML chain handling in the normalized layer
- rust-analyzer's approach: https://rust-analyzer.github.io/blog/2020/09/28/how-to-make-a-language-server.html
