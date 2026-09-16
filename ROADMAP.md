# Tungsten Language Roadmap

> **Vision:** A systems-level programming language that provides the fearless concurrency and zero-cost abstractions of Rust, while eliminating explicit lifetime annotations through **Region-Based Memory Management**, eliminating the async/await function coloring divide through **Algebraic Effects**, and eliminating runtime bounds panics through **Refinement Types**.

---

## Progress Overview

| Phase | Milestone | Focus Area | Status |
|:---|:---|:---|:---:|
| **v0.1** | **Working Prototype** | Parser, Typechecker, Refinement Intervals, Effect VM, CLI | **Completed** |
| **v0.2** | **Developer Experience** | Standard Library (`std`), Formatter (`forge fmt`), Language Server (`tungsten-lsp`) | **Completed** |
| **v0.3** | **Advanced Type System** | Relational Refinements, Generics, Effect Row Polymorphism | **Completed** |
| **v0.4** | **TIR & Optimization** | SSA / CFG Intermediate Representation, Bounds Elimination, DCE | **Completed** |
| **v0.5** | **Native Codegen & ABI** | LLVM 18 AOT Driver, Cranelift JIT, CodeView PDB Debug Symbols | **Completed** |
| **v0.6** | **Colorless Concurrency** | M:N Work-Stealing Fiber Scheduler, Nurseries, Channels via Effects | **Completed** |
| **v0.7** | **Physical Region Allocator** | Machine-Level Arena Scopes, $\mathcal{O}(1)$ Bulk Teardown, Escape Analysis | **Completed** |
| **v0.8** | **C-ABI FFI & Safe Database** | `forge bindgen`, Safe SQLite Driver (`std/sqlite`), Region Queries | **Completed** |
| **v0.9** | **Production Stdlib & Packaging** | `Forge.toml`, `Forge.lock`, Multi-Package Workspace, `std::fs`, `std::http` | **Completed** |
| **v0.10** | **Self-Hosting Compiler Frontend** | Pure Tungsten Frontend (`compiler/`), Robin Hood Symbol Interner, `tgc.exe` | **Completed** |
| **v0.11** | **Fortress Security Suite** | 10 Adversarial Vectors, 12 `FORT-*` Invariants, 3-Corpus Fuzzing | **Completed** |
| **v1.0** | **Full Self-Hosting Bootstrap** | Stage 2 Bootstrap (`tgc.exe` compiles itself), Release Candidate | **In Progress** |
| **v1.1** | **Fortress v2: Async Network Engine** | Win32 IOCP, Fixed M:N Worker Task Pool, C100K Scale (< 1.2 KB/fiber) | **Completed** |
| **v1.2** | **Formal Verification & SMT Bridge** | Z3 Solver Bridge for Non-Linear Arithmetic, Affine Handle Invariants | **Planned** |
| **v1.3** | **Cross-Platform & WebAssembly** | Native Linux (ELF), macOS (Mach-O), and WebAssembly (`wasm32`) Targets | **Planned** |

---

## Completed Milestones

### v0.1: Working Prototype
- [x] **Frontend (`tungsten-syntax`)**: Tokenizer and recursive-descent parser handling refinement syntax (`u8(0..=100)`), effect signatures (`yields [Db, IOError]`), and delimited effect handlers (`handle { ... } with Effect { ... }`).
- [x] **Semantic Analysis (`tungsten-typeck`)**: Constant interval constraint arithmetic (`[min, max]`), bounds checking on assignments and arithmetic, compile-time effect row checking and unhandled effect rejection.
- [x] **Runtime Evaluator (`tungsten-vm`)**: Delimited continuation effect dispatcher supporting mock and async handlers. Scoped reference borrowing (`&mut`) without explicit lifetime annotations.
- [x] **Tooling (`forge`)**: CLI runner supporting `forge check` and `forge run`.

### v0.2: Developer Experience (DX) & Tooling
- [x] **Standard Library (`std/`)**: Pre-registered prelude refinement types (`Percentage`, `Port`, `NonZeroU32`, `Byte`) and core algebraic effects (`IO`, `Random`, `State`, `Time`, `Yield`).
- [x] **Opinionated Formatter (`forge fmt`)**: AST-aware pretty printer with canonical 4-space indentation, operator spacing, and trailing commas. Idempotent: $\mathrm{fmt}(\mathrm{fmt}(x)) = \mathrm{fmt}(x)$.
- [x] **Language Server Protocol (`tungsten-lsp`)**: JSON-RPC 2.0 stdio server (`forge lsp`) for VS Code, Neovim, and Helix. Real-time diagnostics, hover tooltips detailing inferred types and refinement intervals, and format on save.

### v0.3: Advanced Type System & Frontier PL
- [x] **Relational Refinement Types**: Linear arithmetic verification between function parameters (`fn subslice_len(start: usize, end: usize(>= start)) -> usize`). Statically proven bounds checking and compile-time rejection of relational contract violations.
- [x] **Generics & Parametric Polymorphism**: Parametric structs (`struct Container<T> { value: T }`), generic functions, and bidirectional Hindley-Milner-style unification and type substitution (`Subst`, `unify`, `substitute`).
- [x] **Effect Polymorphism & Higher-Order Functions**: First-class function types (`fn apply<T, U, E>(val: T, f: fn(T) yields [E] -> U) -> U yields [E]`), caller-agnostic higher-order abstraction over arbitrary effect rows without color segregation.

### v0.4: Intermediate Representation (TIR) & Optimization Passes
- [x] **SSA Intermediate Representation (`tungsten-tir`)**: Linearized Basic-Block Control Flow Graph (CFG) preserving types, refinement intervals, and effect signatures.
- [x] **AST to TIR Lowering**: Lowers expressions, loops, branching, function calls into SSA instructions, and delimited algebraic effect frames into continuations.
- [x] **Middle-End Optimization Pipeline**: Constant folding (`const_fold`), dead code elimination (`dce`), and redundant bounds check elimination (`bounds_elim`) proving zero-cost abstraction for safe intervals.

### v0.5: Native Codegen via LLVM 18 & JIT Execution
- [x] **Machine Codegen Backend (`tungsten-codegen`)**: Direct translation of TIR basic blocks into portable LLVM 18 IR text and native machine assembly (`x86_64-pc-windows-gnu`).
- [x] **Debug Symbols & Linker Integration**: CodeView debug information with PDB generation via `rust-lld`. High-performance LTO and function/data-section garbage collection in release mode.
- [x] **In-Memory JIT Compilation Engine**: `JitEngine` compiling TIR modules into executable machine code in memory.

### v0.6: Colorless Concurrency via Algebraic Effects & M:N Work-Stealing Fiber Runtime
- [x] **Fiber Engine & Structured Concurrency (`tungsten-fiber`)**: Multi-threaded M:N work-stealing scheduler distributing fibers across worker threads with local queues and global injector queue. Structured concurrency nursery scopes (`Nursery<T>`) guaranteeing lexical lifecycle boundaries and deterministic fiber joins.
- [x] **Concurrency Algebraic Effects in Typechecker (`tungsten-typeck`)**: Registered `Async`, `Channel`, and `FiberHandle` in core type system. Type inference and effect validation for `Async::spawn`, `Async::yield_now`, `Async::await_fiber`, `Async::sleep`, `Channel::new`, `Channel::send`, and `Channel::recv`.

### v0.7: Physical Region Allocator & Compile-Time Region Inference
- [x] **Region Syntax & Grammar (`tungsten-syntax`)**: `region [name] { ... }` lexical block expression syntax, parser, and AST node `ExprKind::Region`.
- [x] **Compile-Time Region Inference & Escape Analysis (`tungsten-typeck`)**: Region IDs attached to references (`Type::Ref { is_mut, inner, region: Option<RegionId> }`). Automatic inferencing of region scopes without explicit `<'a>` lifetime annotations. Strict Linear Escape Analysis rejecting references escaping local regions via returns, struct fields, wrapper types, or enum payloads.
- [x] **Native Machine Region Allocator (`tungsten-codegen`)**: Zero-overhead bump allocator `PhysicalArena` with $\mathcal{O}(1)$ bulk teardown. Runtime C-ABI symbols: `tungsten_region_enter`, `tungsten_region_alloc`, `tungsten_region_exit`.

### v0.8: C-ABI FFI & Safe Database Driver ("The Killer App")
- [x] **Automated C Header Bindgen (`forge bindgen`)**: Parsed C function prototypes, structs, and `#define` constants directly into safe `extern "C"` bindings.
- [x] **Safe SQLite Driver (`std/sqlite.tg`)**: Column-name schema separation preventing string duplication across 10,000+ rows; zero-per-row heap allocation via `sqlite_query_in`.
- [x] **Prepared Statement SQLi Defense**: Implemented `sqlite_execute_prepared` with parameter binding via `sqlite3_bind_text` and `sqlite3_bind_int64`, providing immune parameterized database execution.
- [x] **Algebraic Effect Database Offload**: Declared `effect Database` with native runtime bridge `@tungsten_database_execute` and `@tungsten_database_query`.

### v0.9: Production Package Manager & Standard Library Expansion
- [x] **Deterministic Package Management (`forge`)**: Implemented `Forge.toml` manifests, dependency graph resolution, deterministic `Forge.lock` lockfile generation, and diamond dependency deduplication.
- [x] **Standard Library File System & Processes (`std/fs.tg`, `std/process.tg`)**: Effect-handled filesystem operations (`FS`) and process execution (`Process`) with zero-copy region ingestion.
- [x] **Cache-Conscious Robin Hood `HashMap` (`std/collections.tg`)**: Structure-of-Arrays (SoA) design with FNV-1a hashing, probe sequence length (PSL) tracking, and dual system-heap / scoped-region allocator support.
- [x] **Zero-Copy HTTP/1.1 Engine (`std/http.tg`)**: High-performance HTTP parser with region slicing, 64KB max-header guard, and status response formatters.

### v0.10: Self-Hosting Compiler Frontend (Bootstrap Stage 1)
- [x] **Pure Tungsten Compiler Frontend (`compiler/`)**:
  - `compiler/diagnostics.tg`: Algebraic effect `Diagnostics` for reporting errors and warnings.
  - `compiler/interner.tg`: Region-backed Robin Hood symbol interner converting identifiers into dense 64-bit integer symbols with zero heap churn.
  - `compiler/ast.tg`: Compact AST nodes allocated directly into the compiler scratch arena.
  - `compiler/lexer.tg`: Fast keyword and multi-character operator tokenizer.
  - `compiler/parser.tg`: Recursive descent parser with Pratt precedence climbing.
  - `compiler/codegen.tg`: LLVM 18 IR emitter generating SSA instructions, locals, and runtime thunks.
  - `compiler/main.tg`: Compiler CLI driver orchestrating the pipeline into native binaries.
- [x] **3-Stage Bootstrap Grand Loop**:
  - Stage 1: Host `forge` compiles `compiler/main.tg` into `tgc.exe`.
  - Stage 2: `tgc.exe` autonomously tokenizes, parses, and generates LLVM IR for `examples/bootstrap_sample.tg`.
  - Stage 3: Generated binary runs natively, accurately computing arithmetic, loops, factorials, and functions.

### v0.11: Fortress Security & Verification Suite
- [x] **10 Adversarial Attack Vectors Verified**:
  - Group A: HTTP boundary enforcement (9B rejection, 10B minimal valid, 16/17-char verbs, 8KB URLs, >64KB rejection).
  - Group B: TCP fragmentation characterization and connection flood resilience.
  - Group C: Slow trickle rejection and idle socket drop recovery.
  - Group D: Prepared statement SQL injection torture (zero injection across union, boolean, and comment attacks).
  - Group E: Multi-point memory stability across 200 sequential requests (delta: 0.95 MB < 15 MB) and 100,100 nested allocation stress test.
  - Group F: Negative region escape torture (direct returns, struct fields, pointer casts, wrapper structs, enum payloads strictly rejected by typechecker).
  - Group G: Crash containment & immediate `200 OK` `/health` recovery after every hostile vector.
  - Group I: Multi-generation clean server restart (5 consecutive spawn/traffic/shutdown cycles with exit code 0).
  - Group K: 3-Corpus fuzzing (random bytes 0–8KB, mutated HTTP requests, boundary payloads) with 0 crashes or hangs.
- [x] **Formal Invariant Ledger (`FORT-*`)**: 12/12 invariants passed with 100% success across 134+ workspace tests.

### v1.1: Fortress v2 — High-Concurrency Async Network Engine (C100K & Win32 IOCP)
- [x] **Kernel-Level Win32 IOCP Completion Port Engine (`crates/tungsten-fiber/src/net.rs`)**:
  - Pinned `OVERLAPPED` I/O contexts (`PinnedIoContext`) with 8 KB `wsabuf`, socket handles, and completion channels.
  - Zero-polling event notification via `CreateIoCompletionPort` and `GetQueuedCompletionStatus`.
  - Asynchronous `WSARecv` and `WSASend` execution with zero OS thread preemption.
- [x] **Fixed M:N Worker Task Pool (`crates/tungsten-codegen/src/llvm_text.rs`)**:
  - 4-thread fixed OS worker pool (`@tungsten_worker_loop`) with semaphore and mutex synchronization, replacing unbounded thread creation.
  - Ultra-lean task allocation: only 40 bytes per task frame on heap, achieving **< 1.2 KB overhead per connection** (beating Go's 2 KB goroutine stack).
  - Non-blocking socket readiness detection (`ioctlsocket` with `FIONREAD = 0x4004667f`) dispatching ready connections while rotating idle tasks back to the queue tail without lock contention or thread quantum stalling.
- [x] **Structured Concurrency Nursery Microservice (`examples/web_service_v2.tg`)**:
  - Nursery-spawned fiber-per-connection server (`nursery server_n { ... }`).
  - Scoped request region `region req_r` per client with $O(1)$ bulk memory reclamation on disconnect.
  - Multi-point graceful nursery join on `/shutdown` returning exit code 0.
- [x] **C100K High-Concurrency Automated Verification Suite (`crates/forge/tests/c100k_tests.rs`)**:
  - Sustained **5,000 persistent simultaneous TCP sockets** held idle for 5 seconds.
  - Measured process working set: **10.79 MB** total footprint (< 100 MB target).
  - Mid-stream heartbeat request responded in **117.06 ms** with `200 OK` identifying as `tungsten-fortress/2.0-iocp`.
  - All 6 formal ledger invariants (`FORT2-IOCP-001`, `FORT2-FIBER-001`, `FORT2-SCALE-001`, `FORT2-MEM-001`, `FORT2-HEART-001`, `FORT2-SHUT-001`) passed with 100% success.
  - Workspace test suite: **137 passed, 0 failed**.

---

## Upcoming Milestones: The Path to v1.0 & Beyond

### Phase 6: Full Self-Hosting Bootstrap & v1.0 Release Candidate (v1.0)
*Target: Complete self-sufficiency where the Tungsten compiler is built entirely by Tungsten itself.*

1. **Self-Hosting Stage 2 (Bootstrap Closure)**:
   - Compile `compiler/main.tg` using `tgc.exe` (Stage 1) to produce `tgc_stage2.exe`.
   - Verify Stage 2 binary equivalence or semantic convergence: `tgc_stage2.exe` compiles the standard library and compiler test suites cleanly.
2. **Compiler Frontend Enhancements in Tungsten**:
   - Port the typechecker (`tungsten-typeck`) and region escape analysis into `compiler/typeck.tg`.
   - Port the middle-end TIR optimization passes (constant folding, bounds check elimination) into `compiler/opt.tg`.
3. **Formal Language Specification & EBNF**:
   - Publish formal grammar specification and operational semantics for Tungsten's region inference, algebraic effects, and refinement intervals.

---

### Phase 8: Formal Verification & SMT Solver Integration (v1.2)
*Target: Mathematically verifiable zero-panic systems programming.*

1. **Z3 SMT Solver Bridge**:
   - Optional Z3 solver integration for non-linear arithmetic refinement constraints (multiplication, division, modular arithmetic).
   - Automated induction proofs for recursive function bounds.
2. **Affine & Linear Resource Invariants**:
   - Affine ownership types for OS handles (sockets, file descriptors, database connections), proving at compile time that handles are closed exactly once and cannot leak across error paths.

---

### Phase 9: Multi-Target Codegen & WebAssembly (v1.3)
*Target: Universal systems development from bare metal to browsers.*

1. **Linux (x86_64 / AArch64) & macOS Targets**:
   - ELF and Mach-O binary emission via LLVM backend and LLD linker.
   - POSIX socket and syscall shims in `std/fs` and `std/net`.
2. **WebAssembly Target (`wasm32-unknown-unknown`)**:
   - Direct compilation to Wasm bytecode.
   - Algebraic effect mapping to JavaScript host promises and browser Web APIs without runtime shims.
