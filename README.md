<div align="center">

# 🌅 Morrow

### *Next-Generation Systems Programming Language*

**Fearless Concurrency &bull; Region-Based Memory &bull; Algebraic Effects &bull; Refinement Proofs**

<br />

<!-- Tier 1: Project Metadata & Quality Assurance Badges -->
<p align="center">
  <a href="https://github.com/LeTrollologist/Morrow/releases">
    <img src="https://img.shields.io/badge/Release-v2.0.2_Standalone-6366F1?style=for-the-badge&logo=github&logoColor=white&labelColor=0f172a" alt="Release" />
  </a>
  <a href="compiler/">
    <img src="https://img.shields.io/badge/Compiler-Pure_Morrow-8B5CF6?style=for-the-badge&logo=llvm&logoColor=white&labelColor=0f172a" alt="Compiler" />
  </a>
  <a href="bin/">
    <img src="https://img.shields.io/badge/Self--Hosting-100%25_Genesis-10B981?style=for-the-badge&logo=git&logoColor=white&labelColor=0f172a" alt="Self-Hosting" />
  </a>
  <a href="ROADMAP.md">
    <img src="https://img.shields.io/badge/Parity-Bitwise_Identical-06B6D4?style=for-the-badge&logo=target&logoColor=white&labelColor=0f172a" alt="Parity" />
  </a>
  <a href="tests/">
    <img src="https://img.shields.io/badge/Tests-21%2F21_Passing-22C55E?style=for-the-badge&logo=checkmarx&logoColor=white&labelColor=0f172a" alt="Tests" />
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/License-MIT_%2F_Apache--2.0-F59E0B?style=for-the-badge&logo=open-source-initiative&logoColor=white&labelColor=0f172a" alt="License" />
  </a>
</p>

<!-- Tier 2: Supported Target Architectures -->
<p align="center">
  <img src="https://img.shields.io/badge/Target-Windows_x86__64-0078D4?style=flat-square&logo=windows&logoColor=white&labelColor=0f172a" alt="Windows x86_64" />
  &nbsp;
  <img src="https://img.shields.io/badge/Target-Linux_ELF_x86__64-FCC624?style=flat-square&logo=linux&logoColor=black&labelColor=0f172a" alt="Linux x86_64" />
  &nbsp;
  <img src="https://img.shields.io/badge/Target-Linux_AArch64-E95420?style=flat-square&logo=ubuntu&logoColor=white&labelColor=0f172a" alt="Linux AArch64" />
  &nbsp;
  <img src="https://img.shields.io/badge/Target-WebAssembly_wasm32-654FF0?style=flat-square&logo=webassembly&logoColor=white&labelColor=0f172a" alt="WebAssembly" />
</p>

<!-- Tier 3: Interactive Quick Navigation Buttons -->
<p align="center">
  <a href="#quickstart-building-from-source">
    <img src="https://img.shields.io/badge/⚡_Quickstart-Run_Morrow-38BDF8?style=for-the-badge&logoColor=white&labelColor=0f172a" alt="Quickstart" />
  </a>
  &nbsp;
  <a href="docs/SYNTAX.md">
    <img src="https://img.shields.io/badge/📖_Language_Tour-Syntax_Guide-818CF8?style=for-the-badge&logoColor=white&labelColor=0f172a" alt="Syntax Guide" />
  </a>
  &nbsp;
  <a href="ROADMAP.md">
    <img src="https://img.shields.io/badge/🗺️_Roadmap-Milestones_&_Vision-34D399?style=for-the-badge&logoColor=white&labelColor=0f172a" alt="Roadmap" />
  </a>
</p>

</div>

---

## Core Differentiators

1. **Region-Based Memory Management (No explicit `<'a>` lifetimes)**  
   Allocations are grouped into compile-time inferred regions (`region r { ... }`). When a region exits scope, memory is reclaimed en masse in $\mathcal{O}(1)$ without lifetime annotations polluting struct and function definitions. Strict linear escape analysis guarantees reference safety.

2. **Colorless Functions via Algebraic Effects**  
   No `async` or `await` infection. Functions declare effect capabilities (`yields [Db, IOError, Diagnostics]`). Delimited handlers with native machine-level continuations (`resume`) support synchronous execution, fiber suspension, or pure unit-test mocking.

3. **Compile-Time Refinement Types**  
   Declare bounded types like `type Health = u8[0..100];` and `type Port = u16[1..65535];`. The checker rejects out-of-range **literal** casts and statically rejects division by a refined divisor whose interval spans zero. Casts from variables are not yet range-proven, and there is no path-sensitive narrowing — see the verified status table in [`docs/SYNTAX.md` §14](docs/SYNTAX.md#14-not-implemented--known-limitations).

4. **Linear & Affine Resource Types (Leak Prevention)**  
   `linear struct` enforces exactly-once consumption, including branch convergence across `if/else`: dropping an unconsumed handle is a compile error. `affine struct` currently follows the same all-paths consumption rule — automatic drop on early `return` is not implemented yet.

5. **Universal Multi-Target Cross-Compilation**  
   Target Windows GNU (`x86_64-pc-windows-gnu`), Linux ELF (`x86_64-unknown-linux-gnu`), Linux AArch64 (`aarch64-unknown-linux-gnu`), and WebAssembly (`wasm32-unknown-unknown`) from a single compiler binary via `--target`.

6. **Deterministic Package Management & Lockfile (`Forge.lock`)**  
   Pure local SemVer resolution (`^`, `*`), 3-state DAG cycle detection, alphabetical lockfile determinism, graph closure verification, and transactional dependency mutation via `forge add` and `forge resolve`.

7. **100% Pure Standalone Self-Hosting Compiler (Independent of Rust, Cargo, & Seed Compilers)**  
   The entire compiler frontend, typechecker, TIR intermediate representation, middle-end optimizer, LLVM code generator, formatter, LSP server, and Forge CLI are written in **pure Morrow** (`compiler/*.mw`). The build path contains no Rust or Cargo — but note that the final link step **does** invoke `clang` on LLVM IR (and uses the MinGW CRT objects in `lib/crt` on Windows), so clang is a runtime requirement of the toolchain.

---

## Verified Reality (v2.0.1)

This README describes what the released compiler **actually does**. The full,
tested breakdown lives in [`docs/SYNTAX.md` §14](docs/SYNTAX.md#14-not-implemented--known-limitations); the short version:

| Area | Status |
|---|---|
| Regions (`region r { ... }`), `_in` allocators, O(1) teardown | ✅ implemented, region bodies yield values |
| Algebraic effects (`effect`, `yields`, `perform`, `handle … with`, `resume`) | ✅ implemented; arms must be `fn op(k, args) { resume(k, v); }` and cover every op |
| Linear resources (`linear struct`, `consume`, branch convergence) | ✅ enforced |
| Affine resources (`affine struct`) | ⚠️ parsed; all-paths consumption required (no auto-drop on early `return`) |
| Refinements | ⚠️ literal casts + divisor-span checks only (no variable proofs, no narrowing) |
| Generics + turbofish (`identity::<i64>(42)`, `Pair::<i64> { … }`) | ✅ implemented |
| Region escape analysis | ❌ not enforced |
| `enum` / `match`, `for`, `nursery`/fibers, closures, `if`-expressions, `continue` | ❌ not implemented (parse errors) |
| Float / hex / underscore literals, `null`, `/* */` comments | ❌ not implemented |
| Debug info (PDB), LTO, section GC | ❌ not implemented (stage-0 Rust only) |
| `println` | one value per call: string literal or integer; **no `{}` interpolation** |
| Call signatures | callee existence + argument **count** enforced (v2.0.2); argument *types* not compared |
| `std::process` | `process_exec` works; `command_new`/`command_arg` link, `command_status` faults (no default `Process` effect handler) |

Toolchain prerequisite: **`clang` on `PATH`** (the compiler emits LLVM IR and
shells out to clang to link; Windows linking also uses the MinGW CRT in `lib/crt`).

---

## Modern Syntax Showcase

```morrow
import std.net;
import std.collections;

// 1. Refinement Types: Bounds proved mathematically at compile-time
type Health = u8[0..100];
type Port = u16[1..65535];

struct Player {
    name: String,
    hp: Health,
}

// 2. Algebraic Effects instead of 'async' or 'Result'
effect Database {
    fn query(sql: String) -> String;
}

fn fetch_player(id: i64) -> Player yields [Database] {
    let record = perform Database.query("SELECT name, hp FROM players WHERE id = 1");
    
    Player {
        name: "Arthur",
        hp: 85 as Health,
    }
}

// 3. Scoped Regions: O(1) bulk memory cleanup
fn process_batch() {
    region batch_reg {
        let mut buffer = string_buffer_new_in(batch_reg);
        string_buffer_push_str(&mut buffer, "Processing region batch...");
    } // Entire arena memory reclaimed instantly here
}

// 4. Delimited Handlers & Clean Entry Point
fn main() -> i64 {
    let p = handle {
        fetch_player(42)
    } with Database {
        fn query(k, sql: String) {
            resume(k, "Arthur");
        }
    };
    0
}
```

---

## Workspace Architecture

```
Morrow/
├── bin/                   # Standalone Production Binaries (Pure Self-Hosted Seed)
│   ├── mwc.exe            # Pure Morrow Native Compiler
│   └── forge.exe          # Pure Morrow Toolchain & Package Manager
├── compiler/              # Self-Hosting Compiler Subsystems (100% Pure Morrow)
│   ├── ast.mw             # AST node data structures and token definitions
│   ├── lexer.mw           # High-speed keyword and symbol tokenizer
│   ├── parser.mw          # Recursive-descent parser with precedence climbing
│   ├── typeck.mw          # Bidirectional typechecker & refinement constraint solver
│   ├── tir.mw             # Basic-Block SSA Typed Intermediate Representation
│   ├── opt.mw             # Constant folding, DCE, and bounds elimination passes
│   ├── smt.mw             # SMT-LIB2 solver bridge (QF_NIA / QF_LIA query generator)
│   ├── codegen.mw         # Multi-target LLVM IR emitter, runtime thunks, and ABI bridge
│   ├── fmt.mw             # Canonical, idempotent pretty-printer
│   ├── package.mw         # Forge.toml manifest parser, SemVer, and Forge.lock solver
│   ├── json.mw            # Pure Morrow JSON serializer and deserializer
│   ├── lsp.mw             # Language Server Protocol stdio RPC engine
│   ├── forge.mw           # Unified CLI toolchain implementation
│   ├── diagnostics.mw     # Algebraic effect diagnostic error reporter
│   ├── interner.mw        # Region-backed Robin Hood symbol interner
│   └── main.mw            # Compiler driver entry point
├── std/                   # Morrow Standard Library (Pure Morrow)
│   ├── collections.mw     # Cache-conscious Robin Hood HashMap, Vec, StringBuffer
│   ├── test.mw            # Native testing framework (assert, assert_eq, assert_str_eq)
│   ├── fs.mw              # Scoped region filesystem operations
│   ├── process.mw         # Subprocess spawning and execution
│   ├── refinements.mw     # Standard mathematical interval refinements
│   ├── io.mw              # Buffered console I/O, BufReader/BufWriter streams
│   ├── net.mw             # TCP socket networking
│   ├── http.mw            # Zero-copy HTTP/1.1 engine
│   ├── crypto.mw          # Hashing, checksums, hex codecs, CSPRNG
│   ├── sql.mw             # High-level SQLite connection & transaction wrapper
│   ├── sql_builder.mw     # Fluent, injection-safe SQL query builder
│   ├── sqlite.mw          # Parameterized, SQL-injection-immune database driver
│   ├── sync.mw            # Channels and message passing
│   └── time.mw            # Monotonic host clock, stopwatch, throughput math
├── tests/                 # Pure Morrow Test Suites (21 Native Suites)
│   ├── typeck_refinements.mw # Type checking and interval bounds verification
│   ├── tir_optimizer.mw   # SSA constant folding and optimization passes
│   ├── formatter_tests.mw # Pretty-printer output and idempotency check
│   ├── package_tests.mw   # SemVer, caret constraints, and Forge.lock generation
│   ├── package_negative_test.mw # Cycle detection, conflict rejection, and closure invariants
│   ├── bootstrap_tests.mw # Recursion, arithmetic, and control flow sanity
│   ├── error_handling_tests.mw # Algebraic effect diagnostic reporting
│   ├── win32_gdi_callback_test.mw # Native Win32 C-ABI FFI and callbacks
│   ├── struct_field_layout_test.mw # Dynamic struct field offsets and indexing
│   ├── refinement_test.mw # User-defined refinement syntax and interval solver
│   ├── region_escape_test.mw # Compile-time lexical region escape analysis
│   ├── generics_test.mw   # Parametric generics and monomorphization
│   ├── effects_test.mw    # Delimited continuations, resumption, and abortive unwinding
│   ├── formal_verification_test.mw # Non-linear SMT intervals, affine & linear resources
│   ├── crypto_test.mw     # Hex codecs, hashes, checksums, deterministic RNG
│   ├── io_test.mw         # Integer formatting, BufReader lines, BufWriter assembly
│   ├── sql_test.mw        # Query builder composition and SQL rendering
│   ├── lsp_test.mw        # JSON-RPC framing, hover, completion, diagnostics
│   ├── http_test.mw       # HTTP/1.1 parser edge cases and response formatting
│   ├── net_test.mw        # std::net effect plumbing against mock handlers
│   └── time_test.mw       # Monotonic clock, stopwatch reset, throughput helper
├── benchmarks/            # Phase 13.3 Runtime Benchmark Suite
│   ├── bench_region_alloc.mw # Region bump allocation vs malloc/free
│   ├── bench_http_parse.mw   # std::http parser throughput
│   ├── bench_hashmap.mw      # Robin Hood HashMap insert/lookup density
│   └── c/                    # Equivalent C baselines (clang -O2, MinGW CRT)
│       ├── bench_region_alloc.c
│       ├── bench_http_parse.c
│       └── bench_hashmap.c
├── examples/              # Flagship Examples & Production Microservices
│   ├── bootstrap_sample.mw# Minimal bootstrap program
│   ├── wasm_sample.mw     # WebAssembly computation demo
│   ├── web_service_v2.mw  # Fortress v2 Async IOCP Web Server (C100K engine)
│   ├── web_service.mw     # Fortress v1 REST API with SQLite & Region Sandboxing
│   └── player.mw          # Game demo with effects, refinements, and regions
├── docs/
│   └── SYNTAX.md          # Comprehensive Language & Syntax Specification
├── LICENSE                # Dual license: MIT OR Apache-2.0 (SPDX)
├── LICENSE-MIT            # MIT license text
├── LICENSE-APACHE         # Apache-2.0 license reference
└── archive/               # Historical Stage-0 Artifacts
    └── stage0-rust/       # Archived legacy Rust bootstrap compiler crates
```

---

## Developer Experience & Tooling

All developer workflows are executed natively through [`bin/forge.exe`](bin/forge.exe) and [`bin/mwc.exe`](bin/mwc.exe) without any Cargo or Rust dependency.

### 1. Run Native Test Suites (`forge test`)
Run all 21 pure Morrow test suites compiled and executed on the fly, followed by
the compile-fail corpus in `tests/negative/` (15 programs that must be rejected
with a specific diagnostic):
```powershell
.\bin\forge.exe test
```
```
running morrow native test suites...
=== Running Typecheck & Refinement Test Suite === ... ok
=== Running TIR & Optimizer Test Suite === ... ok
=== Running Formatter Test Suite === ... ok
=== Running Package Resolver Test Suite === ... ok
=== Running Package Negative Test Suite === ... ok
=== Running Bootstrap Sanity Test Suite === ... ok
=== Running Error Handling & Diagnostics Test Suite === ... ok
=== Running Win32 FFI & Callbacks Test Suite === ... ok
=== Running Dynamic Struct Field Layout & Indexing Test Suite === ... ok
=== Running Refinement Interval Arithmetic Test Suite === ... ok
=== Running Region Escape Analysis Test Suite === ... ok
=== Running Parametric Generics & Monomorphization Test Suite === ... ok
=== Running Delimited Algebraic Effects Test Suite === ... ok
=== Running Formal Verification & Linear Resources Test Suite === ... ok
=== Running std::crypto Test Suite === ... ok
=== Running std::io Test Suite === ... ok
=== Running std::sql_builder Test Suite === ... ok
=== Running Phase 13.1 LSP Harness Test Suite === ... ok
=== Running std::http Test Suite === ... ok
=== Running std::net Test Suite === ... ok
=== Running std::time Test Suite === ... ok
running morrow negative (compile-fail) suites...
test result: ok. all test suites passed!
```

### 2. Compile a Program (`forge build` or `mwc`)
```powershell
# Compile natively for host (Windows x86_64)
.\bin\forge.exe build examples\bootstrap_sample.mw -o target\app.exe

# Or compile directly with the self-hosted compiler
.\bin\mwc.exe examples\bootstrap_sample.mw -o target\app.exe
```

### 3. Universal Cross-Compilation (`--target`)
Compile from Windows to Linux ELF, 64-bit ARM, or WebAssembly:
```powershell
# Linux x86_64 ELF
.\bin\mwc.exe examples\bootstrap_sample.mw --target x86_64-unknown-linux-gnu -o target\app_linux

# Linux AArch64 (64-bit ARM)
.\bin\mwc.exe examples\bootstrap_sample.mw --target aarch64-unknown-linux-gnu -o target\app_arm64.o

# WebAssembly (runnable via Node.js or browser)
.\bin\mwc.exe examples\wasm_sample.mw --target wasm32-unknown-unknown -o target\app.wasm
```

### 4. Package Management & Deterministic Locking (`forge add`, `forge resolve`)
```powershell
# Add a local dependency with SemVer constraints
.\bin\forge.exe add math_lib --path ../math_lib --version ^1.2.0

# Resolve dependency DAG and generate Forge.lock
.\bin\forge.exe resolve

# Check/verify existing Forge.lock graph closure
.\bin\forge.exe lock
```

### 5. Rapid Type & Refinement Check (`forge check`)
Perform instantaneous lexical, syntactical, refinement interval, and region escape checking without code generation:
```powershell
.\bin\forge.exe check examples\player.mw
```

### 6. Canonical Code Formatting (`forge fmt`)
```powershell
# Format code in place
.\bin\forge.exe fmt examples\bootstrap_sample.mw

# Validate formatting in CI
.\bin\forge.exe fmt --check examples\bootstrap_sample.mw
```
The formatter is idempotent (formatting twice is a no-op, and `--check` passes on
its own output). Note that the repository's existing sources are not yet written in
that canonical style, so running `--check` across the tree reports every file as
unformatted — format files before enabling `--check` as a CI gate.

### 7. Self-Compiling the Compiler (Fixed-Point Bootstrap Loop)
Recompile the entire Morrow compiler using its own binary:
```powershell
.\bin\mwc.exe compiler\main.mw -o target\stage1.exe
.\target\stage1.exe compiler\main.mw -o target\stage2.exe
.\target\stage2.exe compiler\main.mw -o target\stage3.exe
```
This produces bitwise identical LLVM IR (`SHA256(stage2.exe.ll) == SHA256(stage3.exe.ll)`), proving mathematical compiler stability.

### 8. Runtime Benchmarking (`forge bench`)
Compile and run every benchmark in `benchmarks/`, then build and run the
equivalent C baselines in `benchmarks/c/` with `clang -O2` so Morrow's zero-cost
claims can be checked against hand-written C on the same machine:
```powershell
.\bin\forge.exe bench
```
Representative output (best of three runs on a busy developer host, v2.0.1):
```
running morrow runtime benchmarks...
=== Benchmark: benchmarks/bench_region_alloc.mw ===
=== Morrow Benchmark: Region Allocator vs malloc ===
BENCH morrow_region_bump iterations=1000000 elapsed_ms=17 ops_per_sec=58823529
BENCH morrow_malloc_free iterations=1000000 elapsed_ms=27 ops_per_sec=37037037
=== Benchmark: benchmarks/bench_http_parse.mw ===
=== Morrow Benchmark: std::http Parser Throughput ===
BENCH morrow_http_parse iterations=200000 elapsed_ms=206 ops_per_sec=970873
=== Benchmark: benchmarks/bench_hashmap.mw ===
=== Morrow Benchmark: HashMap Insert/Lookup Density ===
BENCH morrow_hashmap_insert iterations=200000 elapsed_ms=54 ops_per_sec=3703703
BENCH morrow_hashmap_lookup iterations=200000 elapsed_ms=31 ops_per_sec=6451612
=== C baselines (clang -O2) ===
=== C Baseline: malloc/free vs bump allocator ===
BENCH c_malloc_free iterations=1000000 elapsed_ms=27 ops_per_sec=37037037
BENCH c_bump_alloc iterations=1000000 elapsed_ms=14 ops_per_sec=71428571
=== C Baseline: HTTP/1.1 request parser ===
BENCH c_http_parse iterations=200000 elapsed_ms=7 ops_per_sec=28571428
=== C Baseline: open-addressing hash table ===
BENCH c_hashmap_insert iterations=200000 elapsed_ms=19 ops_per_sec=10526315
BENCH c_hashmap_lookup iterations=200000 elapsed_ms=8 ops_per_sec=25000000
bench result: ok. all benchmark programs completed.
```

Every benchmark program is an ordinary Morrow program, so it can also be run
individually and instrumented with the `std::time` stopwatch:
```powershell
.\bin\forge.exe run benchmarks\bench_hashmap.mw
```

Each result line is machine-readable
(`BENCH <name> iterations=<n> elapsed_ms=<n> ops_per_sec=<n>`), so the suite can
be captured in CI logs and diffed across compiler revisions.

