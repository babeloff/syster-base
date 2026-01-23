# Syster Base

Core library for SysML v2 and KerML parsing, AST, and semantic analysis.

## Features

- **Parser**: Pest-based grammar for SysML v2 and KerML
- **AST**: Complete abstract syntax tree types
- **Incremental Semantic Analysis**: Salsa-powered query system with automatic memoization
- **Name Resolution**: Scope-aware resolver with import handling
- **Standard Library**: SysML v2 standard library files

## Architecture

Syster Base uses a **query-based incremental computation** model powered by [Salsa](https://github.com/salsa-rs/salsa). This means:

- **Automatic memoization** — Query results are cached; re-running a query with the same inputs returns instantly
- **Automatic invalidation** — When an input changes, only dependent queries recompute
- **Parallel-safe** — Salsa's design enables safe concurrent query execution

### Query Layers

```
file_text(file)           ← INPUT: raw source text
    │
    ▼
parse_file(file)          ← Parse into AST (memoized per-file)
    │
    ▼
file_symbols(file)        ← Extract HIR symbols (memoized per-file)
    │
    ▼
SymbolIndex               ← Workspace-wide symbol index
    │
    ▼
Resolver::resolve(name)   ← Name resolution with imports
    │
    ▼
file_diagnostics(file)    ← Semantic errors
```

### Key Types

| Type | Size | Purpose |
|------|------|---------|
| `FileId` | 4 bytes | Interned file identifier (O(1) comparison) |
| `Name` | 4 bytes | Interned string identifier |
| `DefId` | 8 bytes | Globally unique definition ID |
| `HirSymbol` | — | Symbol extracted from AST |
| `RootDatabase` | — | Salsa database holding all queries |

## Usage

```rust
use syster::hir::{RootDatabase, FileText, parse_file, file_symbols};
use syster::base::FileId;

// Create the Salsa database
let db = RootDatabase::new();

// Set file content (input query)
let file_id = FileId::new(0);
let file_text = FileText::new(&db, file_id, r#"
    package Vehicle {
        part def Car {
            attribute mass : Real;
        }
    }
"#.to_string());

// Parse (memoized - subsequent calls are instant)
let parse_result = parse_file(&db, file_text);
assert!(parse_result.is_ok());

// Extract symbols (also memoized)
if let Some(ast) = parse_result.get_ast() {
    let symbols = file_symbols(file_id, ast);
    // symbols contains: Vehicle (package), Car (part def), mass (attribute)
}
```

## Modules

- `base` — Foundation types: `FileId`, `Name`, `Interner`, `TextRange`
- `syntax` — Pest grammars and AST types for KerML/SysML
- `hir` — High-level IR with Salsa queries and symbol extraction
- `ide` — IDE features: completion, goto, hover, references
- `project` — File loading utilities

## Performance

The Salsa-based architecture provides significant performance benefits:

- **Incremental parsing**: Only changed files are re-parsed
- **Memoized queries**: Symbol extraction, resolution cached automatically
- **O(1) comparisons**: Interned `FileId` and `Name` enable constant-time equality
- **Reduced allocations**: String interning shares storage across the codebase

## License

MIT

## Development

### DevContainer Setup (Recommended)

This project includes a DevContainer configuration for a consistent development environment.

**Using VS Code:**
1. Install the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)
2. Open this repository in VS Code
3. Click "Reopen in Container" when prompted (or use Command Palette: "Dev Containers: Reopen in Container")

**What's included:**
- Rust 1.85+ with 2024 edition
- rust-analyzer, clippy
- GitHub CLI
- All VS Code extensions pre-configured

### Manual Setup

If not using DevContainer:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the project
cargo build --release

# Run tests
cargo test --release

# Run clippy (required before commit)
cargo clippy --all-targets -- -D warnings
```
