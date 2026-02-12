# Proptest Roundtrip Plan: External SysML Sources

Plan for property-based roundtrip tests that exercise the full
`.sysml` -> HIR -> Model -> serialized format -> Model pipeline
against real-world SysML files drawn from public repositories.

## Goal

Verify that arbitrary `.sysml` files from the wild survive
serialization roundtrips across all supported representations
without data loss.  The representations under test are:

| Format | Description | Roundtrip path |
|--------|-------------|----------------|
| **SysML** | Textual notation (`.sysml`) | Model -> `decompile()` -> SysML text -> parse -> HIR -> `model_from_symbols()` -> Model |
| **XMI** | XML Model Interchange | Model -> `Xmi::write()` -> bytes -> `Xmi::read()` -> Model |
| **YAML** | YAML interchange | Model -> `Yaml::write()` -> bytes -> `Yaml::read()` -> Model |
| **JSON-LD** | JSON Linked Data | Model -> `JsonLd::write()` -> bytes -> `JsonLd::read()` -> Model |
| **KPAR** | ZIP archive (XMI + metadata) | Model -> `Kpar::write()` -> bytes -> `Kpar::read()` -> Model |
| **AST** | Rowan syntax tree | SysML text -> `SyntaxFile::sysml()` -> AST -> extract symbols -> `model_from_symbols()` -> Model |

Tests select a randomized subset of files from three external
sources, record which samples were used and when, and let the
operator control the sample count.

---

## 1. External Sources

| Repo | URL | SysML file locations |
|------|-----|----------------------|
| SysML-v2-Release | `https://github.com/Systems-Modeling/SysML-v2-Release` | `sysml/`, `sysml.library/` |
| SysML-v2-Pilot-Implementation | `https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation` | `sysml/`, `sysml.library/` |
| GfSE/SysML-v2-Models | `https://github.com/GfSE/SysML-v2-Models` | `models/` |

### Acquisition strategy

Follow the existing pattern in `test_official_xmi_roundtrip.rs`:

1. Check well-known local paths first (`target/test-data/<repo>/`, `/tmp/<repo>/`).
2. If not found, shallow-clone with sparse checkout of only the SysML
   subdirectories listed above.
3. Skip gracefully (not fail) if `git` is unavailable or the network is down.

Repos are cloned once under `target/test-data/` and reused across runs.
They are excluded from version control by the existing `target/` entry
in `.gitignore`.

---

## 2. Sample Selection and Recording

### 2.1 File discovery

For each available repo, recursively collect all paths matching
`**/*.sysml`.  Produce a single combined `Vec<SourceFile>`:

```rust
struct SourceFile {
    /// Absolute local path to the file.
    path: PathBuf,
    /// Which repo it came from (for logging/recording).
    repo: &'static str,
    /// Path relative to repo root (stable across machines).
    relative_path: String,
}
```

### 2.2 Randomized sampling

Use `proptest`'s built-in random number generation (`TestRunner`
and its `Rng`) to select a subset of files.  This ensures that
proptest's seed controls the selection, making failures
reproducible via the persisted seed.

The **sample count** is controlled by an environment variable:

```
PROPTEST_SYSML_SAMPLES=20  cargo test --features interchange,proptest \
    --test test_proptest_sysml_roundtrip
```

Default: `10`.  Setting `0` means "use all files" (useful for
exhaustive CI runs).

### 2.3 Usage recording

Each test run appends to a **sample log** at
`target/test-data/sysml-sample-log.jsonl` (JSON Lines format,
one record per line).  The file accumulates across runs.

```jsonl
{"timestamp":"2026-02-12T14:30:00Z","repo":"SysML-v2-Release","file":"sysml/Vehicle.sysml","url":"https://github.com/Systems-Modeling/SysML-v2-Release/blob/main/sysml/Vehicle.sysml","test":"yaml_roundtrip","result":"pass"}
{"timestamp":"2026-02-12T14:30:00Z","repo":"GfSE/SysML-v2-Models","file":"models/CoffeeMachine/CoffeeMachine.sysml","url":"https://github.com/GfSE/SysML-v2-Models/blob/main/models/CoffeeMachine/CoffeeMachine.sysml","test":"xmi_roundtrip","result":"fail","error":"Element count mismatch: 12 vs 11"}
```

Fields:
- `timestamp` -- ISO 8601 UTC.
- `repo` -- Short repo identifier.
- `file` -- Relative path within repo.
- `url` -- Reconstructed GitHub blob URL (for human reference).
- `test` -- Which roundtrip test was run.
- `result` -- `"pass"` or `"fail"`.
- `error` -- Error message (only on failure).

#### Reuse throttling

Before selecting a file, check the log for recent entries.
A file is **deprioritized** (not excluded) if it was used within the
last N runs (configurable via `PROPTEST_SYSML_REUSE_WINDOW`,
default `5`).  Deprioritized files are placed at the end of the
candidate list so fresh files are preferred, but they can still be
selected if there are fewer candidates than the requested sample
count.

---

## 3. Failure Capture

When a roundtrip test fails for a sample file, the test
infrastructure copies the faulty `.sysml` source into a
`./failures/` directory at the project root.  This directory
is **checked into version control** so that failures are
visible in code review and can be used as regression fixtures.

### 3.1 Directory layout

```
failures/
  sysml-v2-release/
    sysml/
      Vehicle.sysml                 # copy of the faulty source
      Vehicle.sysml.failure.json    # failure metadata
  sysml-v2-pilot/
    sysml/
      SomeExample.sysml
      SomeExample.sysml.failure.json
  gfse-sysml-v2-models/
    models/
      CoffeeMachine/
        CoffeeMachine.sysml
        CoffeeMachine.sysml.failure.json
```

The directory structure mirrors the relative path within the
source repo so that files with the same name from different
subdirectories don't collide.

### 3.2 Failure metadata

Each `.failure.json` sidecar records what went wrong:

```json
{
  "timestamp": "2026-02-12T14:30:00Z",
  "repo": "SysML-v2-Release",
  "url": "https://github.com/Systems-Modeling/SysML-v2-Release/blob/main/sysml/Vehicle.sysml",
  "relative_path": "sysml/Vehicle.sysml",
  "test": "xmi_roundtrip",
  "error": "Element count mismatch: 12 vs 11",
  "serialized_output": "base64-or-path-to-artifact"
}
```

Fields:
- `timestamp` -- When the failure was recorded.
- `repo` -- Source repository identifier.
- `url` -- GitHub blob URL for the original file.
- `relative_path` -- Path within the source repo.
- `test` -- Which roundtrip test failed.
- `error` -- The comparison error message.
- `serialized_output` -- Optional.  For small models, the
  serialized bytes (base64) that produced the mismatch.
  For large models, omitted to keep the file manageable.

### 3.3 Lifecycle

- **On failure**: The `.sysml` file and its `.failure.json`
  sidecar are written to `./failures/`.  If a failure for the
  same file and test already exists, it is overwritten with
  the latest result.
- **On fix**: Once the underlying bug is resolved and the file
  passes all roundtrip tests, the developer manually deletes
  the entry from `./failures/` and commits the removal.
  This provides a clear signal in the commit history.
- **As regression tests**: CI can optionally re-run the files
  in `./failures/` as targeted regression tests, independent
  of the randomized sampling.  A separate test function
  `test_failure_regressions()` iterates over `./failures/`,
  runs all roundtrip tests, and reports which ones still fail
  vs. which have been fixed.

### 3.4 Implementation

```rust
fn record_failure(
    source_file: &SourceFile,
    test_name: &str,
    error: &str,
    serialized: Option<&[u8]>,
) {
    let failures_dir = Path::new("./failures").join(&source_file.repo_dir);
    let dest = failures_dir.join(&source_file.relative_path);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();

    // Copy the .sysml source
    fs::copy(&source_file.path, &dest).unwrap();

    // Write the sidecar metadata
    let meta_path = dest.with_extension("sysml.failure.json");
    let meta = serde_json::json!({
        "timestamp": now_iso8601(),
        "repo": source_file.repo,
        "url": source_file.github_url(),
        "relative_path": source_file.relative_path,
        "test": test_name,
        "error": error,
    });
    fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();
}
```

---

## 4. Roundtrip Pipeline Under Test

Each sampled `.sysml` file goes through the following pipeline:

```
.sysml source text
    |
    v
[1] Parse via AnalysisHost::set_file_content()
    |
    v
[2] Extract HirSymbol[] via SymbolIndex
    |
    v
[3] Convert to Model via model_from_symbols()
    |
    +-----> [4a] Serialize to format X via Format::write()
    |             |
    |             v
    |       [4b] Deserialize from format X via Format::read()
    |             |
    |             v
    |       [4c] Compare Model (step 3) vs Model (step 4b)
    |
    +-----> [5a] decompile() -> SysML text
    |             |
    |             v
    |       [5b] SyntaxFile::sysml() -> AST -> extract -> model_from_symbols()
    |             |
    |             v
    |       [5c] Compare Model (step 3) vs Model (step 5b)
    |
    +-----> [6a] Model -> Kpar::write() -> bytes -> Kpar::read() -> Model
                  |
                  v
            [6b] Compare Model (step 3) vs Model (step 6a)
```

### 4.1 What is compared

Use the same `models_equivalent()` comparison logic from the
existing `test_proptest_roundtrip.rs`, with `CompareMode::Coercing`
for XMI/KPAR (both XML-based, attributes are untyped strings) and
`CompareMode::Strict` for YAML/JSON-LD.

For the SysML and AST roundtrips, comparison is at the semantic
level: element kinds, names, boolean flags, and relationship
structure.  Formatting and comments are not preserved by
`decompile()`, so byte-level comparison is not expected.

- Element count
- Element IDs preserved
- Element kinds preserved
- Element names and short names preserved
- Boolean flags preserved
- Custom property values preserved (with type coercion for XMI/KPAR)
- Relationship count and identity (YAML/JSON-LD only)

### 4.2 Tests to run per sample

For each sampled file, run these roundtrip tests:

| Test name | Pipeline | Compare mode |
|-----------|----------|--------------|
| `sysml_roundtrip` | Model -> decompile -> SysML text -> parse -> HIR -> Model | Semantic |
| `ast_roundtrip` | SysML text -> SyntaxFile -> AST -> extract symbols -> Model -> decompile -> SysML text -> SyntaxFile -> AST -> extract symbols -> Model | Semantic |
| `xmi_roundtrip` | Model -> XMI -> Model | Coercing |
| `yaml_roundtrip` | Model -> YAML -> Model | Strict |
| `jsonld_roundtrip` | Model -> JSON-LD -> Model | Strict |
| `kpar_roundtrip` | Model -> KPAR -> Model | Coercing |
| `xmi_to_yaml` | Model -> XMI -> Model -> YAML -> Model | Coercing |
| `yaml_to_jsonld` | Model -> YAML -> Model -> JSON-LD -> Model | Strict |
| `sysml_to_xmi` | Model -> decompile -> parse -> Model -> XMI -> Model | Coercing |

The `sysml_roundtrip` and `ast_roundtrip` tests verify the full
cycle through the textual SysML representation and the concrete
syntax tree.  The `kpar_roundtrip` test exercises the ZIP-based
archive format, which wraps XMI internally.

### 4.3 Parse-failure tolerance

Some `.sysml` files from external repos may use syntax not yet
supported by the parser.  Files that produce parse errors are
**recorded** in the sample log with `result: "skip"` and a
reason, then excluded from the roundtrip comparison.  This
prevents false negatives from parser limitations while still
tracking coverage.

---

## 5. Test File Structure

### 5.1 New file

`tests/test_proptest_sysml_roundtrip.rs`

Gated on both features:
```rust
#![cfg(all(feature = "interchange", feature = "proptest"))]
```

### 5.2 Module layout

```rust
// --- Configuration ---
const DEFAULT_SAMPLE_COUNT: usize = 10;
const DEFAULT_REUSE_WINDOW: usize = 5;

// --- Source repos ---
struct RepoSource { name, url, clone_path, sysml_dirs }

// --- File discovery ---
fn find_or_clone_repos() -> Vec<RepoSource>
fn discover_sysml_files(repos: &[RepoSource]) -> Vec<SourceFile>

// --- Sample selection ---
fn select_samples(
    files: &[SourceFile],
    count: usize,
    log: &SampleLog,
    reuse_window: usize,
    rng: &mut impl Rng,
) -> Vec<&SourceFile>

// --- Sample log ---
struct SampleLog { entries: Vec<SampleEntry> }
struct SampleEntry { timestamp, repo, file, url, test, result, error }
impl SampleLog {
    fn load(path: &Path) -> Self
    fn append(&self, entry: SampleEntry)
    fn recent_uses(&self, file: &str, window: usize) -> usize
}

// --- Roundtrip pipeline ---
fn parse_sysml_to_model(source: &str) -> Result<Model, String>
fn roundtrip_format(model: &Model, format: &dyn ModelFormat, mode: CompareMode) -> Result<(), String>
fn roundtrip_chain(model: &Model, f1: &dyn ModelFormat, f2: &dyn ModelFormat, mode: CompareMode) -> Result<(), String>
fn roundtrip_sysml(model: &Model) -> Result<(), String>     // decompile -> parse -> compare
fn roundtrip_ast(source: &str) -> Result<(), String>         // parse -> model -> decompile -> parse -> model -> compare
fn roundtrip_kpar(model: &Model) -> Result<(), String>       // Kpar::write -> Kpar::read -> compare

// --- Failure capture ---
fn record_failure(source_file: &SourceFile, test_name: &str, error: &str, serialized: Option<&[u8]>)

// --- Regression runner ---
fn test_failure_regressions()  // re-runs files in ./failures/

// --- proptest integration ---
// Use a single proptest with a custom strategy that:
// 1. Discovers files (cached)
// 2. Selects a random index into the file list
// 3. Runs all roundtrip tests for that file
// 4. Records failures to ./failures/ with sidecar metadata
```

### 5.3 proptest integration approach

Rather than using `proptest!` macro with a model strategy (which
generates synthetic models), use `TestRunner` directly to select
file indices.  This gives control over the RNG while still
benefiting from proptest's seed persistence and shrinking:

```rust
#[test]
fn sysml_file_roundtrip() {
    let files = discover_all_sysml_files();
    if files.is_empty() {
        eprintln!("No SysML files found, skipping");
        return;
    }

    let sample_count = env_var_or("PROPTEST_SYSML_SAMPLES", DEFAULT_SAMPLE_COUNT);
    let reuse_window = env_var_or("PROPTEST_SYSML_REUSE_WINDOW", DEFAULT_REUSE_WINDOW);
    let log = SampleLog::load(&log_path());

    let mut runner = TestRunner::default();
    let samples = select_samples(&files, sample_count, &log, reuse_window, runner.rng());

    for source_file in &samples {
        let content = std::fs::read_to_string(&source_file.path).unwrap();
        let model = match parse_sysml_to_model(&content) {
            Ok(m) => m,
            Err(reason) => {
                log.append(skip_entry(source_file, &reason));
                continue;
            }
        };

        let mut any_failed = false;
        for (test_name, test_fn) in roundtrip_tests() {
            let result = test_fn(&model);
            log.append(result_entry(source_file, test_name, &result));
            if let Err(e) = &result {
                record_failure(source_file, test_name, e, None);
                eprintln!("FAIL {}: {} -- {}", source_file.relative_path, test_name, e);
                any_failed = true;
            }
        }
        if any_failed {
            failures.push(source_file.relative_path.clone());
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} file(s) failed roundtrip tests (see ./failures/):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
}
```

---

## 6. Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PROPTEST_SYSML_SAMPLES` | `10` | Number of `.sysml` files to test per run. `0` = all. |
| `PROPTEST_SYSML_REUSE_WINDOW` | `5` | Number of past runs to check for recent use of a file. |

---

## 7. CI Considerations

- **Fast CI**: `PROPTEST_SYSML_SAMPLES=5` for quick smoke testing.
- **Nightly CI**: `PROPTEST_SYSML_SAMPLES=0` for exhaustive coverage.
- The `target/test-data/` clones can be cached in CI to avoid
  re-cloning on every run.
- The sample log at `target/test-data/sysml-sample-log.jsonl`
  should **not** be cached in CI (each run starts fresh).
  It is only useful for local development to track coverage
  over time.

---

## 8. Implementation Steps

1. **Add repo cloning infrastructure** to the test file,
   reusing the pattern from `test_official_xmi_roundtrip.rs`.
   Support all three repos with their respective SysML subdirectories.

2. **Implement file discovery** with recursive `.sysml` collection
   and `SourceFile` metadata.

3. **Implement the sample log** (JSONL append, recent-use query).

4. **Implement the parse pipeline** (`parse_sysml_to_model`) using
   `AnalysisHost` and `model_from_symbols`, with graceful handling
   of parse failures.

5. **Wire up format roundtrip tests** using the comparison
   infrastructure already in `test_proptest_roundtrip.rs`.
   Import or duplicate `models_equivalent()`, `CompareMode`, etc.
   Implement `roundtrip_format()` for XMI, YAML, JSON-LD;
   `roundtrip_kpar()` for KPAR; and the cross-format chains.

6. **Add the SysML roundtrip** (`roundtrip_sysml`): calls
   `decompile()` to produce SysML text, parses it back via
   `AnalysisHost`, extracts symbols, converts to Model, and
   compares at the semantic level.

7. **Add the AST roundtrip** (`roundtrip_ast`): parses the
   original `.sysml` text to a `SyntaxFile` (rowan AST),
   extracts symbols, builds a Model, decompiles back to text,
   re-parses, re-extracts, re-builds, and compares the two
   Models.  This exercises the full concrete syntax tree path.

8. **Implement failure capture** (`record_failure`) that copies
   the faulty `.sysml` and writes a `.failure.json` sidecar
   into `./failures/<repo>/<relative_path>`.

9. **Implement `test_failure_regressions`** that re-runs all
   roundtrip tests against files already in `./failures/`,
   reporting which are fixed and which still fail.

10. **Implement sample selection** with deprioritization of
    recently used files.

11. **Verify** locally with `PROPTEST_SYSML_SAMPLES=3` against
    each repo individually, then all together.
