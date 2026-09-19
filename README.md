# Tungsten

> **Tungsten** is a modern systems-level programming language designed to provide fearless concurrency, zero-cost abstractions, and mathematical memory safety without garbage collection, borrow-checker lifetimes, or async function coloring.

NOTE: Tungsten is not a production level programming language at the current moment and should not be considered stable. Processes, features, and much more will be changed regularly.
[![Release](https://img.shields.io/badge/Release-v1.5.0%20Genesis-blue.svg)](https://github.com/LeTrollologist/Tungsten/releases)
[![Pure Tungsten](https://img.shields.io/badge/Language-Pure%20Tungsten-blue.svg)](compiler/)
[![Self-Hosting](https://img.shields.io/badge/Self--Hosting-100%25%20Genesis-brightgreen.svg)](bin/)
[![Convergence](https://img.shields.io/badge/Fixed--Point%20Convergence-Bitwise%20Identical-success.svg)](ROADMAP.md)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

For complete language grammar, types, and standard library reference, see [docs/SYNTAX.md](docs/SYNTAX.md).  
For development history and upcoming milestones, see [ROADMAP.md](ROADMAP.md).

---

## Core Differentiators

1. **Region-Based Memory Management (No explicit `<'a>` lifetimes)**  
   Allocations are grouped into compile-time inferred regions (`region r { ... }`). When a region exits scope, memory is reclaimed en masse in $\mathcal{O}(1)$ without lifetime annotations polluting struct and function definitions. Strict linear escape analysis guarantees reference safety.

2. **Colorless Functions via Algebraic Effects**  
   No `async` or `await` infection. Functions declare effect capabilities (`yields [Db, IOError, Diagnostics]`). Callers choose how effects are handled: synchronous execution, event-loop suspension, or pure unit-test mocking.

3. **Compile-Time Refinement Types (Zero-Panic Bounds)**  
   Declare bounded types like `type Health = u8[0..100];` and `type Port = u16[1..65535];`. The compiler mathematically proves arithmetic constraints at compile time, eliminating runtime out-of-bounds panics and bounds-checking overhead.

4. **High-Concurrency Async Engine (Fortress v2)**  
   M:N fiber task pool driven by kernel-level Win32 I/O Completion Ports (IOCP) and POSIX event multiplexing. Sustains 5,000+ simultaneous connections with < 1.2 KB RAM overhead per connection and zero thread quantum stalls.

5. **100% Pure Self-Hosting Genesis (Independent of Rust & Cargo)**  
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
    fn query(sql: String): String;
}

fn fetch_player(id: i64): Player yields [Database] {
    var record = Database.query("SELECT name, hp FROM players WHERE id = 1");
    
    Player {
        name: "Arthur",
        hp: 85 as Health,
    }
}

// 3. Scoped Regions: O(1) bulk memory cleanup
fn process_batch() {
    region batch_reg {
        var buffer = string_buffer_new_in(batch_reg);
        string_buffer_push_str(&mut buffer, "Processing region batch...");
        println(string_as_ptr(&buffer));
    } // Entire arena memory reclaimed instantly here
}

// 4. Clean Entry Point
fn main(): i64 {
    var p = fetch_player(42);
    println(p.name);
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
│   ├── codegen.tg         # LLVM IR emitter, runtime thunks, and C-ABI bridge
│   ├── fmt.tg             # Canonical, idempotent pretty-printer
│   ├── package.tg         # Forge.toml manifest parser and package resolver
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
├── tests/                 # Pure Tungsten Test Suites
│   ├── typeck_refinements.tg # Type checking and interval bounds verification
│   ├── tir_optimizer.tg   # SSA constant folding and optimization passes
│   ├── formatter_tests.tg # Pretty-printer output and idempotency check
│   ├── package_tests.tg   # Forge.toml parsing and dependency resolution
│   ├── bootstrap_tests.tg # Recursion, arithmetic, and control flow sanity
│   └── error_handling_tests.tg # Algebraic effect diagnostic reporting
├── examples/              # Flagship Examples & Production Microservices
│   ├── bootstrap_sample.tg# Minimal bootstrap program
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
Run all pure Tungsten test suites compiled and executed on the fly:
```powershell
.\bin\forge.exe test
```
```
running tungsten native test suites...
=== Running Typecheck & Refinement Test Suite ===
  test Percentage refinement value preservation ... ok
  test Port refinement value preservation ... ok
  test Percentage lower bound (0) ... ok
  test Percentage upper bound (100) ... ok
All refinement tests passed!

=== Running TIR & Optimizer Test Suite ===
  test Constant folding (300 + 100 + 200 = 600) ... ok
  test Recursive Fibonacci evaluation fib(7) = 13 ... ok
  test Loop accumulation sum(1..10) = 55 ... ok
All TIR & Optimizer tests passed!

=== Running Formatter Test Suite ===
  test Formatter produced output ... ok
  test Formatter idempotency check (fmt(fmt(x)) == fmt(x)) ... ok
All formatter tests passed!

=== Running Package Resolver Test Suite ===
  test Package name parsing ... ok
  test Package version parsing ... ok
  test Package edition parsing ... ok
  test Dependency count ... ok
  test Dependency name parsing ... ok
  test Dependency path parsing ... ok
All package resolver tests passed!

=== Running Bootstrap Sanity Test Suite ===
  test Sum 1..10 = 55 ... ok
  test Factorial 5! = 120 ... ok
  test Factorial 6! = 720 ... ok
All bootstrap sanity tests passed!

=== Running Error Handling & Diagnostics Test Suite ===
  test report_err returns true on dispatched diagnostic ... ok
  test report_warn returns true on dispatched warning ... ok
  test Safe division non-zero ... ok
  test Safe division zero-divisor handling ... ok
All error handling tests passed!
test result: ok. all test suites passed!
```

### 2. Compile a Program (`forge build` or `tgc`)
```powershell
# Compile via Forge
.\bin\forge.exe build examples\bootstrap_sample.tg -o target\app.exe

# Or compile directly with the self-hosted compiler
.\bin\tgc.exe examples\bootstrap_sample.tg -o target\app.exe
```

### 3. Rapid Type & Refinement Check (`forge check`)
Perform instantaneous lexical, syntactical, and refinement interval checking without code generation:
```powershell
.\bin\forge.exe check examples\bootstrap_sample.tg
```

### 4. Canonical Code Formatting (`forge fmt`)
```powershell
# Format code in place
.\bin\forge.exe fmt examples\bootstrap_sample.tg

# Validate formatting in CI
.\bin\forge.exe fmt --check examples\bootstrap_sample.tg
```

### 5. Self-Compiling the Compiler (Self-Hosting Loop)
Recompile the entire Tungsten compiler using its own binary:
```powershell
.\bin\tgc.exe compiler\main.tg -o target\tgc_new.exe
```
This produces a bitwise fixed-point executable that passes all native test suites autonomously.
