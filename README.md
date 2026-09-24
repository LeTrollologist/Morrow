# Tungsten

> **Tungsten** is a modern systems-level programming language designed to provide fearless concurrency, zero-cost abstractions, and mathematical memory safety without garbage collection, borrow-checker lifetimes, or async function coloring.

NOTE: Tungsten is not a production level programming language at the current moment and should not be considered stable. Processes, features, and much more will be changed regularly.
[![Release](https://img.shields.io/badge/Release-v2.0.0-blue.svg)](https://github.com/LeTrollologist/Tungsten/releases)
[![Pure Tungsten](https://img.shields.io/badge/Language-Pure%20Tungsten-blue.svg)](compiler/)
[![Self-Hosting](https://img.shields.io/badge/Self--Hosting-100%25%20Genesis-brightgreen.svg)](bin/)
[![Convergence](https://img.shields.io/badge/Fixed--Point%20Convergence-Bitwise%20Identical-success.svg)](ROADMAP.md)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

For complete language grammar, types, and standard library reference, see [docs/SYNTAX.md](docs/SYNTAX.md).  
For development history and upcoming milestones, see [ROADMAP.md](ROADMAP.md).  
For the architecture audit and remediation matrix, see [AUDIT.md](AUDIT.md).

---

## Core Differentiators

1. **Region-Based Memory Management (No explicit `<'a>` lifetimes)**  
   Allocations are grouped into compile-time inferred regions (`region r { ... }`). When a region exits scope, memory is reclaimed en masse in $\mathcal{O}(1)$ without lifetime annotations polluting struct and function definitions. Strict linear escape analysis guarantees reference safety.

2. **Colorless Functions via Algebraic Effects**  
   No `async` or `await` infection. Functions declare effect capabilities (`yields [Db, IOError, Diagnostics]`). Delimited handlers with native machine-level continuations (`resume`) support synchronous execution, fiber suspension, or pure unit-test mocking.

3. **Compile-Time Refinement Types & SMT Verification**  
   Declare bounded types like `type Health = u8[0..100];` and `type Port = u16[1..65535];`. The compiler mathematically proves arithmetic constraints at compile time, eliminating runtime out-of-bounds panics. Path-sensitive interval narrowing and non-linear interval arithmetic prevent division by zero.

4. **Linear & Affine Resource Types (Leak Prevention)**  
   `linear struct` enforces exactly-once consumption with branch convergence analysis across `if/else`, eliminating resource leaks. `affine struct` guarantees at-most-once consumption with deterministic auto-drop at scope exit.

5. **Universal Multi-Target Cross-Compilation**  
   Target Windows GNU (`x86_64-pc-windows-gnu`), Linux ELF (`x86_64-unknown-linux-gnu`), Linux AArch64 (`aarch64-unknown-linux-gnu`), and WebAssembly (`wasm32-unknown-unknown`) from a single compiler binary via `--target`.

6. **Deterministic Package Management & Lockfile (`Forge.lock`)**  
   Pure local SemVer resolution (`^`, `*`), 3-state DAG cycle detection, alphabetical lockfile determinism, graph closure verification, and transactional dependency mutation via `forge add` and `forge resolve`.

7. **100% Pure Self-Hosting Genesis (Independent of Rust & Cargo)**  
   The entire compiler frontend, typechecker, TIR intermediate representation, middle-end optimizer, LLVM code generator, formatter, LSP server, and Forge CLI are written in **pure Tungsten** (`compiler/*.tg`). Standalone binaries (`bin/tgc.exe`, `bin/forge.exe`) provide instant compilation with zero external toolchain dependencies.

---

## Modern Syntax Showcase

```tungsten
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
Tungsten/
├── bin/                   # Standalone Production Binaries (Pure Self-Hosted Seed)
│   ├── tgc.exe            # Pure Tungsten Native Compiler
│   └── forge.exe          # Pure Tungsten Toolchain & Package Manager
├── compiler/              # Self-Hosting Compiler Subsystems (100% Pure Tungsten)
│   ├── ast.tg             # AST node data structures and token definitions
│   ├── lexer.tg           # High-speed keyword and symbol tokenizer
│   ├── parser.tg          # Recursive-descent parser with precedence climbing
│   ├── typeck.tg          # Bidirectional typechecker & refinement constraint solver
│   ├── tir.tg             # Basic-Block SSA Typed Intermediate Representation
│   ├── opt.tg             # Constant folding, DCE, and bounds elimination passes
│   ├── smt.tg             # SMT-LIB2 solver bridge (QF_NIA / QF_LIA query generator)
│   ├── codegen.tg         # Multi-target LLVM IR emitter, runtime thunks, and ABI bridge
│   ├── fmt.tg             # Canonical, idempotent pretty-printer
│   ├── package.tg         # Forge.toml manifest parser, SemVer, and Forge.lock solver
│   ├── json.tg            # Pure Tungsten JSON serializer and deserializer
│   ├── lsp.tg             # Language Server Protocol stdio RPC engine
│   ├── forge.tg           # Unified CLI toolchain implementation
│   ├── diagnostics.tg     # Algebraic effect diagnostic error reporter
│   ├── interner.tg        # Region-backed Robin Hood symbol interner
│   └── main.tg            # Compiler driver entry point
├── std/                   # Tungsten Standard Library (Pure Tungsten)
│   ├── collections.tg     # Cache-conscious Robin Hood HashMap, Vec, StringBuffer
│   ├── test.tg            # Native testing framework (assert, assert_eq, assert_str_eq)
│   ├── fs.tg              # Scoped region filesystem operations
│   ├── process.tg         # Subprocess spawning and execution
│   ├── refinements.tg     # Standard mathematical interval refinements
│   ├── io.tg              # Buffered console I/O
│   ├── net.tg             # TCP socket networking
│   ├── http.tg            # Zero-copy HTTP/1.1 engine
│   ├── sqlite.tg          # Parameterized, SQL-injection-immune database driver
│   └── sync.tg            # Channels and message passing
├── tests/                 # Pure Tungsten Test Suites (14 Native Suites)
│   ├── typeck_refinements.tg # Type checking and interval bounds verification
│   ├── tir_optimizer.tg   # SSA constant folding and optimization passes
│   ├── formatter_tests.tg # Pretty-printer output and idempotency check
│   ├── package_tests.tg   # SemVer, caret constraints, and Forge.lock generation
│   ├── package_negative_test.tg # Cycle detection, conflict rejection, and closure invariants
│   ├── bootstrap_tests.tg # Recursion, arithmetic, and control flow sanity
│   ├── error_handling_tests.tg # Algebraic effect diagnostic reporting
│   ├── win32_gdi_callback_test.tg # Native Win32 C-ABI FFI and callbacks
│   ├── struct_field_layout_test.tg # Dynamic struct field offsets and indexing
│   ├── refinement_test.tg # User-defined refinement syntax and interval solver
│   ├── region_escape_test.tg # Compile-time lexical region escape analysis
│   ├── generics_test.tg   # Parametric generics and monomorphization
│   ├── effects_test.tg    # Delimited continuations, resumption, and abortive unwinding
│   └── formal_verification_test.tg # Non-linear SMT intervals, affine & linear resources
├── examples/              # Flagship Examples & Production Microservices
│   ├── bootstrap_sample.tg# Minimal bootstrap program
│   ├── wasm_sample.tg     # WebAssembly computation demo
│   ├── web_service_v2.tg  # Fortress v2 Async IOCP Web Server (C100K engine)
│   ├── web_service.tg     # Fortress v1 REST API with SQLite & Region Sandboxing
│   └── player.tg          # Game demo with effects, refinements, and regions
├── docs/
│   └── SYNTAX.md          # Comprehensive Language & Syntax Specification
└── archive/               # Historical Stage-0 Artifacts
    └── stage0-rust/       # Archived legacy Rust bootstrap compiler crates
```

---

## Developer Experience & Tooling

All developer workflows are executed natively through [`bin/forge.exe`](bin/forge.exe) and [`bin/tgc.exe`](bin/tgc.exe) without any Cargo or Rust dependency.

### 1. Run Native Test Suites (`forge test`)
Run all 14 pure Tungsten test suites compiled and executed on the fly:
```powershell
.\bin\forge.exe test
```
```
running tungsten native test suites...
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
test result: ok. all test suites passed!
```

### 2. Compile a Program (`forge build` or `tgc`)
```powershell
# Compile natively for host (Windows x86_64)
.\bin\forge.exe build examples\bootstrap_sample.tg -o target\app.exe

# Or compile directly with the self-hosted compiler
.\bin\tgc.exe examples\bootstrap_sample.tg -o target\app.exe
```

### 3. Universal Cross-Compilation (`--target`)
Compile from Windows to Linux ELF, 64-bit ARM, or WebAssembly:
```powershell
# Linux x86_64 ELF
.\bin\tgc.exe examples\bootstrap_sample.tg --target x86_64-unknown-linux-gnu -o target\app_linux

# Linux AArch64 (64-bit ARM)
.\bin\tgc.exe examples\bootstrap_sample.tg --target aarch64-unknown-linux-gnu -o target\app_arm64.o

# WebAssembly (runnable via Node.js or browser)
.\bin\tgc.exe examples\wasm_sample.tg --target wasm32-unknown-unknown -o target\app.wasm
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
.\bin\forge.exe check examples\player.tg
```

### 6. Canonical Code Formatting (`forge fmt`)
```powershell
# Format code in place
.\bin\forge.exe fmt examples\bootstrap_sample.tg

# Validate formatting in CI
.\bin\forge.exe fmt --check examples\bootstrap_sample.tg
```

### 7. Self-Compiling the Compiler (Fixed-Point Bootstrap Loop)
Recompile the entire Tungsten compiler using its own binary:
```powershell
.\bin\tgc.exe compiler\main.tg -o target\stage1.exe
.\target\stage1.exe compiler\main.tg -o target\stage2.exe
.\target\stage2.exe compiler\main.tg -o target\stage3.exe
```
This produces bitwise identical LLVM IR (`SHA256(stage2.exe.ll) == SHA256(stage3.exe.ll)`), proving mathematical compiler stability.
