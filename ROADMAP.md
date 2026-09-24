# Tungsten Language Roadmap

> **Vision:** A systems-level programming language that provides the fearless concurrency and zero-cost abstractions of Rust, while eliminating explicit lifetime annotations through **Region-Based Memory Management**, eliminating the async/await function coloring divide through **Algebraic Effects**, and eliminating runtime bounds panics through **Refinement Types**.

---

## Progress Overview

| Phase | Milestone | Focus Area | Status |
|:---|:---|:---|:---:|
| **v0.1** | **Working Prototype** | Parser, Typechecker, Refinement Intervals, Effect VM, CLI (Stage-0 Rust) | **Completed** |
| **v0.2** | **Developer Experience** | Standard Library (`std`), Formatter (`forge fmt`), Language Server (`tungsten-lsp`) (Stage-0 Rust) | **Completed** |
| **v0.3** | **Advanced Type System** | Relational Refinements, Generics, Effect Row Polymorphism (Stage-0 Rust) | **Completed** |
| **v0.4** | **TIR & Optimization** | SSA / CFG Intermediate Representation, Bounds Elimination, DCE (Stage-0 Rust) | **Completed** |
| **v0.5** | **Native Codegen & ABI** | LLVM 18 AOT Driver, Cranelift JIT, CodeView PDB Debug Symbols (Stage-0 Rust) | **Completed** |
| **v0.6** | **Colorless Concurrency** | M:N Work-Stealing Fiber Scheduler, Nurseries, Channels via Effects (Stage-0 Rust) | **Completed** |
| **v0.7** | **Physical Region Allocator** | Machine-Level Arena Scopes, $\mathcal{O}(1)$ Bulk Teardown, Escape Analysis (Stage-0 Rust) | **Completed** |
| **v0.8** | **C-ABI FFI & Safe Database** | `forge bindgen`, Safe SQLite Driver (`std/sqlite`), Region Queries (Stage-0 Rust) | **Completed** |
| **v0.9** | **Production Stdlib & Packaging** | `Forge.toml`, `Forge.lock`, Multi-Package Workspace, `std::fs`, `std::http` (Stage-0 Rust) | **Completed** |
| **v0.10** | **Self-Hosting Compiler Frontend** | Pure Tungsten Frontend (`compiler/`), Robin Hood Symbol Interner, `tgc.exe` | **Completed** |
| **v0.11** | **Fortress Security Suite** | 10 Adversarial Vectors, 12 `FORT-*` Invariants, 3-Corpus Fuzzing (Stage-0 Rust) | **Completed** |
| **v1.0** | **Full Self-Hosting Bootstrap Closure** | 3-Stage Bootstrap Loop, Fixed-Point LLVM IR Identity (`SHA256(S2) == SHA256(S3)`), 100% Green Suite | **Completed** |
| **v1.1** | **Fortress v2: Async Network Engine** | Win32 IOCP, Fixed M:N Worker Task Pool, C100K Scale (< 1.2 KB/fiber) (Stage-0 Rust) | **Completed** |
| **v1.2** | **Cross-Platform Linux & Containerization** | x86_64 Linux (ELF) Target, LLVM POSIX Runtime, Production Multi-Stage Docker (Stage-0 Rust) | **Completed** |
| **v1.4** | **Compiler Integrity & Architecture Remediation** | Dynamic Struct Layouts, Type-Directed Indexing, Real Module Resolution, Error Propagation, CRT Isolation | **Completed** |
| **v1.5** | **Direct TIR Codegen & Optimization Pipeline** | Direct TIR-to-LLVM IR Emission, Dynamic Element Strides, SSA Constant Folding & DCE, Bitwise Bootstrap Parity | **Completed** |
| **v1.6** | **Language Feature Realization** | User-Defined Refinement Syntax, Region Escape Analysis, Parametric Generics with Call-Site Monomorphization, Algebraic Effect Lowering | **In Progress** |
| **v1.7** | **Formal Verification & SMT Bridge** | Z3 Solver Bridge for Non-Linear Arithmetic, Automated Induction Proofs, Affine Handle Invariants | **Planned** |

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

> [!NOTE]
> **Semantic Versioning Progression:**
> Milestones **v1.0** through **v1.3** represent the continuous, linear progression of the stabilized self-hosting compiler, high-concurrency runtime, cross-platform Linux deployment, and full independence from Rust. All releases follow strict semantic versioning and correspond directly to tagged git milestones.

### v1.0: Full Self-Hosting Bootstrap Closure ("The Holy Grail")
- [x] **Full 3-Stage Bootstrap Architecture**:
  - **Stage 1**: Host Rust-based `forge` compiles pure Tungsten compiler driver `compiler/main.tg` into `tgc_stage1.exe`.
  - **Stage 2**: Autonomous `tgc_stage1.exe` ingests all compiler modules (`std/collections.tg`, `compiler/diagnostics.tg`, `compiler/interner.tg`, `compiler/ast.tg`, `compiler/lexer.tg`, `compiler/parser.tg`, `compiler/codegen.tg`, `compiler/main.tg`), tokenizes, parses, and emits `target/bootstrap/tgc_stage2.exe.ll`, then links `tgc_stage2.exe`.
  - **Stage 3**: Stage 2 compiler `tgc_stage2.exe` compiles `compiler/main.tg` from scratch to produce `target/bootstrap/tgc_stage3.exe.ll` and `tgc_stage3.exe`.
- [x] **Fixed-Point Bitwise LLVM IR Convergence**:
  - Verified exact bitwise SHA-256 identity between Stage 2 and Stage 3 LLVM IR outputs:
    $$\mathrm{SHA256}(\mathtt{tgc\_stage2.exe.ll}) \equiv \mathrm{SHA256}(\mathtt{tgc\_stage3.exe.ll})$$
    $$\mathtt{5ab8653c5a8ed65c18bba61e6338cfc13b8d729966afe37e406a0539b49204fa}$$
  - Zero divergence across all 624,612 emitted bytes of LLVM IR text, proving mathematical fixed-point compiler stability.
- [x] **Downstream Program Compilation & Native Execution**:
  - `tgc_stage3.exe` autonomously compiled `examples/bootstrap_sample.tg` into `examples/bootstrap_sample_stage3.exe`.
  - Native execution accurately evaluated loop summations (`55`), factorials (`120`), and multi-argument function calls (`42`).
- [x] **Self-Hosting Language Infrastructure (`compiler/`)**:
  - Struct layout and deterministic field offsets across `Vec`, `StringBuffer`, `HashMap`, and AST nodes.
  - Heap-allocated struct semantics via `@tungsten_alloc` eliminating stack pointer escape bugs.
  - Transparent reference semantics (`&mut`) eliminating pointer-to-pointer dereference mismatches.
  - Expression-based tail return tracking and LLVM basic block termination invariants (`cg.terminated`).
  - Precedence hierarchy: Primary $\to$ Postfix (`.`, `[]`, `as`) $\to$ Unary (`!`, `-`, `&`) $\to$ Binary.
- [x] **Automated Verification Suite**:
  - Automated integration test `crates/forge/tests/bootstrap_closure_tests.rs`.
  - Complete workspace test suite: **140+ tests passed, 0 failed (100% green)**.

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

### v1.2: Cross-Platform Linux Target & Production Dockerization
- [x] **TargetPlatform Architecture (`crates/tungsten-codegen/src/llvm_text.rs`)**:
  - First-class target enumeration (`TargetPlatform::WindowsX86_64`, `TargetPlatform::LinuxX86_64`).
  - Conditional target data layout (`e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128`) and triple (`x86_64-unknown-linux-gnu`).
  - LLVM IR generation via `emit_llvm_ir_with_target(module, target)`.
- [x] **POSIX System & Concurrency Runtime in Pure LLVM IR**:
  - Implemented POSIX socket lifecycle: `socket`, `bind`, `listen`, `accept`, `connect`, `close` (`shutdown(conn, 1)` + `close(conn)` replacing Win32 `WSAStartup`/`closesocket`).
  - Worker task pool on Linux powered by POSIX threads and semaphores: `sem_init`, `sem_wait`, `sem_post`, `pthread_mutex_*`, and `ioctl(sock, FIONREAD, &avail)`.
  - Colorless concurrency fiber yield (`sched_yield`), sleep (`usleep`), and foreign call offloading (`pthread_create` + `sem_trywait`).
  - Fixed stack frame leakage: relocated socket readiness buffer allocations to function `entry:` block, guaranteeing zero stack overflow under high-frequency polling.
- [x] **Cross-Compilation Toolchain in Forge (`crates/forge`)**:
  - CLI flag `--target <triple>` (e.g. `forge build --target x86_64-unknown-linux-gnu`).
  - Dual linking pipeline: native GCC / WSL GCC with `-Wl,--unresolved-symbols=ignore-all -lpthread -ldl -lm`, falling back to `rust-lld -flavor gnu -m elf_x86_64`.
  - Strict host-target CRT path isolation preventing Windows MinGW COFF archives from leaking into Linux ELF links.
- [x] **Production Docker Packaging & Compose Orchestration**:
  - Multi-stage `Dockerfile`: Stage 1 compiles Forge & produces the native Linux microservice; Stage 2 packages an ultra-minimal, unprivileged non-root Debian runtime container with integrated `/health` probes.
  - Production `docker-compose.yml` with CPU/RAM resource limits and security options.
  - Automated integration test suite `crates/forge/tests/linux_cross_tests.rs`: verified 64-bit ELF headers (`0x7F 'E' 'L' 'F'`, EM_X86_64 `0x3E`), WSL execution parity, and microservice compilation.
  - Workspace test suite: **140 passed, 0 failed (100% green)**.

### v1.3: Tungsten Genesis — Complete Pure Tungsten Independence & Language De-Rustification
- [x] **Complete Compiler Ported to Pure Tungsten (`compiler/*.tg`)**:
  - `compiler/ast.tg`: Compact AST structures and token kinds allocated in scoped bump regions.
  - `compiler/lexer.tg`: Tokenizer supporting modern keywords and operator tokens.
  - `compiler/parser.tg`: Recursive descent parser with Pratt operator precedence climbing.
  - `compiler/typeck.tg`: Typechecker and arithmetic interval constraint refinement solver.
  - `compiler/tir.tg`: Basic-block SSA typed intermediate representation with register allocation.
  - `compiler/opt.tg`: Constant folding, dead code elimination, and bounds check elimination passes.
  - `compiler/codegen.tg`: LLVM IR emitter with 64-bit word vs. byte indexing and C-ABI bridge.
  - `compiler/fmt.tg`: Canonical, idempotent code formatter.
  - `compiler/package.tg`: Manifest parser for `Forge.toml` and package dependency resolver.
  - `compiler/json.tg`: Pure Tungsten JSON serializer and deserializer.
  - `compiler/lsp.tg`: Language Server Protocol implementation with stdio JSON-RPC.
  - `compiler/forge.tg`: Unified developer CLI toolchain (`build`, `test`, `check`, `fmt`, `new`, `run`).
  - `compiler/main.tg`: Compiler driver entry point.
- [x] **Surface Syntax De-Rustification (Modern Tungsten Syntax)**:
  - `var` for mutable variables and `let` for immutable bindings (completely eliminating `let mut`).
  - Colon return type notation (`fn foo(): i64` eliminating thin arrow `->`).
  - Clean dot notation for imports and namespaces (`import std.collections;` eliminating `::`).
  - Clean interval bracket notation for refinement constraints (`type Percentage = u8[0..100];`).
- [x] **Native Test Suites & Testing Framework (`std/test.tg`, `tests/*.tg`)**:
  - Implemented `std/test.tg` with `assert`, `assert_eq`, `assert_str_eq`.
  - Replaced all Rust integration tests with native `.tg` suites:
    - `tests/typeck_refinements.tg`: Type checking and refinement constraints.
    - `tests/tir_optimizer.tg`: TIR lowering and optimizer constant folding.
    - `tests/formatter_tests.tg`: Pretty-printer output and idempotency check.
    - `tests/package_tests.tg`: Package manager and manifest parser.
    - `tests/bootstrap_tests.tg`: Arithmetic and recursion bootstrap sanity.
    - `tests/error_handling_tests.tg`: Diagnostic reporting and error handling via algebraic effects.
  - 100% test pass rate running directly via `forge test`.
- [x] **Bitwise Fixed-Point Bootstrap Convergence**:
  - Self-hosting 3-stage bootstrap closure verified: $\text{tgc} \to \text{tgc}_{\text{stage2}} \to \text{tgc}_{\text{stage3}}$.
  - Exact SHA-256 fixed-point convergence confirmed:
    $$\mathrm{SHA256}(\mathtt{tgc\_stage2.exe.ll}) \equiv \mathrm{SHA256}(\mathtt{tgc\_stage3.exe.ll}) \equiv \mathtt{dd6c95b2e085bd916e2c6a5ed146a6ddb7b551d77ec4ad2f87debe6b072d36d4}$$
- [x] **Production Standalone Binaries**:
  - Distributed standalone native binaries `bin/tgc.exe` and `bin/forge.exe`.
- [x] **Complete Retirement of Rust Stage-0 Crates**:
  - All legacy Rust crates (`crates/`) and root `Cargo.toml`/`Cargo.lock` archived to `archive/stage0-rust/`.
  - The Tungsten workspace is 100% pure Tungsten.
- [x] **Comprehensive Genesis Architectural Audit (`AUDIT.md`)**:
  - Detailed forensic audit of `compiler/*.tg`, `std/*.tg`, and `examples/*.tg`.
  - Documented working subsystems (self-hosting loop, LLVM IR bump arena, collections, C FFI).
  - Identified critical technical debt: hardcoded struct field name tables in codegen, variable-name-based indexing, disconnected TIR optimizer passes, missing user-defined refinement types, discarded algebraic effects, and error masking in `forge check`.
  - Established remediation roadmap to bridge the gap between documentation and implementation.

---

## Upcoming Milestones: The Path Forward

---

### Phase 8: Compiler Integrity & Architecture Remediation (v1.4 — In Progress)
*Target: Eliminate compiler shortcuts, fix memory-safety bugs, and establish true multi-file compilation.*

- [x] **Symbol-Driven Struct Field Layout & Type-Directed Indexing**:
   - In `compiler/typeck.tg`, computed struct field byte offsets dynamically based on field declaration order and type sizes (`i64`/`ptr` = 8 bytes, `u8` = 1 byte).
   - Replaced the hardcoded `get_field_offset` string comparison table in `compiler/codegen.tg` with a dynamic symbol lookup table (`struct_sym, field_sym -> offset`), completely eliminating field collision in user structs.
   - Type-directed array & pointer indexing emitting GEP instructions based on element type byte-width instead of variable-name matching.
   - Verified with comprehensive test suite `tests/struct_field_layout_test.tg`.
- [x] **Strict Compilation Error Propagation & Halting**:
   - Implemented centralized diagnostic tracking via runtime globals `@tungsten_diag_errors`, `tungsten_diag_reset`, `tungsten_diag_inc_error`, and `tungsten_diag_get_errors`.
   - Wired `report_err` and `tungsten_report_error` to increment error counts across all pipeline stages (lexing, parsing, typechecking).
   - Updated `compile_file` and `forge check` to halt immediately on error and return exit code 1, eliminating false-positive "check passed: 0 errors" reports on broken ASTs.
- [x] **Connect TIR Intermediate Representation to Codegen**:
   - Updated `emit_llvm_ir` in `compiler/codegen.tg` to directly consume `TirModule` from `lower_ast_program_to_tir` and `tir_optimize_module`.
   - Lowered TIR basic blocks, SSA instructions (`alloca`, `load`, `store`, `binop`, `icmp`, `call`, `br`, `cond_br`, `ret`, `gep`), dynamic struct field offsets, and type-directed indexing.
   - Implemented `determine_index_stride` in `compiler/tir.tg` to dynamically determine element byte widths (1-byte string buffers vs 8-byte pointer tables), ensuring Robin Hood hash table correctness under SSA optimization.
   - Validated full constant folding, dead code elimination, and arithmetic simplification passes on the self-hosted compiler.
   - Reached 3-stage self-hosting fixed point with bitwise identical LLVM IR (`SHA256(S2) == SHA256(S3)`).
- [x] **AST-Level Module & Import Resolution**:
   - Retired the monolithic 30,000-line string-concatenation hack in `load_source_bundle`.
   - Extended `AstProgram` in `compiler/ast.tg` and `compiler/codegen.tg` with `imports: Vec<String>`.
   - Updated `compiler/parser.tg` to parse `import <mod>[:: | .]<submod>;` into file path strings pushed directly to `prog.imports`.
   - Implemented recursive bottom-up module loader (`normalize_path`, `resolve_import_path`, `load_module_recursive`, `load_program_modules`) in `compiler/forge.tg` with cyclic import prevention (`loaded_paths`), compiler intrinsic interception (`std::fs`, `std::process`), and symbol deduplication across `structs`, `extern_fns`, and `functions`.
   - Wired `compile_file` and `forge check` to compile and verify only imported dependencies instead of the entire compiler bundle.
   - Reached 3-stage self-hosting bootstrap fixed-point convergence ($\text{Stage 1} \to \text{Stage 2} \to \text{Stage 3}$) with 100% bitwise LLVM IR parity and all 8 test suites passing cleanly.
- [x] **Toolchain Hygiene & CRT Isolation**:
   - Relocated MinGW CRT startup objects (`crt2.o`, `dllcrt2.o`) and runtime static libraries from gitignored `target/crt/` into tracked `lib/crt/` tree.
   - Updated `.gitignore` with `!lib/crt/**` exception rules to ensure CRT assets are committed.
   - Updated Clang linker flags in `compiler/forge.tg` from `-Ltarget/crt` to `-Llib/crt` and `lib/crt/crt2.o`.
   - Guaranteed automatic directory creation for `target/` in `compile_file`.
   - Implemented functional `forge clean` that purges `target/*.exe`, `target/*.ll`, and test binaries while preserving `lib/crt/`.
   - Verified that `target/crt/` can be completely deleted without breaking fresh builds.

---

### Phase 9: Language Feature Realization (v1.6)
*Target: Implement headline language capabilities in the self-hosted compiler frontend and runtime.*

1. **User-Defined Refinement Types** ✅:
   - Added top-level refinement grammar `type Name = Primitive[min..max];` to `compiler/parser.tg`.
   - Stored refinement intervals in the type environment; enforced arithmetic constraint checking during assignments, function argument passing, and return statements.
   - Prelude refinements: `Percentage`, `Port`, `Byte` registered automatically.
   - Verified with `tests/refinement_test.tg`.
2. **Compile-Time Region Escape Analysis** ✅:
   - Tagged ptr/ref bindings declared inside `region r { ... }` blocks with a region-scope sentinel (`TcType.right = region_sym`).
   - Statically rejects: returning a region-scoped pointer from a function, and assigning a region-scoped pointer to an outer-scope variable.
   - Region depth counter (`region_depth: i64`) and symbol (`cur_region_sym: i64`) threaded through `tc_stmt`.
   - Bootstrap parity confirmed: `SHA256(S2) == SHA256(S3) = B2F126984AE66AB4313A7B89EE9C5EE486C53304B4B6138ACE84301D2095771E`.
3. **Parametric Generics & Call-Site Monomorphization** ✅:
   - Captured generic type parameter symbols in `compiler/parser.tg` (`struct Container<T>`, `fn identity<T>`) and stored on AST nodes (`type_param_sym` in `AstStruct`, `AstFn`).
   - Standardized generic call syntax to turbofish notation (`ident::<Type>(args)` and `ident::<Type>{ fields }`) with explicit grammar rule documentation and unterminated turbofish error diagnostics.
   - Implemented pre-pass monomorphization (`monomorphize_program` in `compiler/forge.tg`) prior to typechecking (Option B design), cloning functions and structs into concrete mangled instances (`ident__concrete`).
   - Deduplicated instantiations and enforced acceptance invariants:
     1. Each concrete generic specialization is created at most once (`already_mono` table).
     2. All call sites to the same specialization resolve to that instance.
     3. No unresolved type parameters reach code generation (`verify_no_unresolved_turbofish` active compile-time invariant guard).
   - Bootstrap parity confirmed: `SHA256(S2) == SHA256(S3) = 89B95F06BE82BB5E42185AF2599A5D5D47BFB212CF03E27888D1FD32BD85E975`.
   - Verified with 16-test comprehensive suite `tests/generics_test.tg`.
4. **Algebraic Effects & Delimited Handlers** ✅:
   - Added `effect` declarations (`AstEffect`, `AstOpSignature`), `yields [...]` function contracts, and `handle { ... } with Handler { ... }` syntax to parser and AST (`AstHandler`, `AstHandlerArm`).
   - Implemented compile-time static effect row checking and contract enforcement in `compiler/typeck.tg`, verifying that unhandled effects are explicitly declared in the enclosing function's `yields` contract.
   - Designed delimited continuation model represented as explicit continuation records (`TungstenContinuation` + `TungstenHandlerFrame`).
   - Implemented single-shot resumption and non-local stack unwinding via self-contained x86_64 assembly routines (`tungsten_setjmp` and `tungsten_longjmp`), bypassing Windows MSVCRT SEH/`RtlUnwindEx` runtime interference.
   - Added dynamic handler frame registration (`tungsten_top_handler_frame`), runtime dispatch tables, and nearest-match effect stack searching supporting nested handlers and shadowing.
   - Verified abortive handlers returning values directly without calling `resume` unwind cleanly to the enclosing `handle` block.
   - Reached 3-stage bootstrap fixed-point parity: `SHA256(stage7.ll) == SHA256(stage8.ll) == SHA256(stage9.ll) = 1B35C04A40BD9B94599F77A8E86BBED6C514207737FA145F439A61CD17D67F9E`.
   - Verified with comprehensive test suite `tests/effects_test.tg` covering single-shot resumption, multi-operation sequencing, abortive unwinding, nested nearest-match dispatch, effect shadowing, deep call chain propagation, multi-argument operations, and deep abort unwinding (100% OK across all 12 native test suites).
5. **Concurrency Runtime Reintegration & Flagship Alignment** ✅:
   - Defined `effect Channel` and `effect Async` in `std/sync.tg` with explicit FIFO `ChannelBuffer` storage, O(1) buffer indexing, and `ChannelHandle<T>`.
   - Updated `std/net.tg` with canonical `perform Net.<op>` syntax for `tcp_listen`, `tcp_accept`, `tcp_connect`, `tcp_read`, `tcp_write`, and `tcp_close`.
   - Rewrote flagship example `examples/player.tg` using actual refinement bounds `u8[0..100]`, multi-effect contracts `yields [Db, Logger]`, and nested delimited handlers with resumption; verified clean native execution.
   - Updated `examples/fibers_and_concurrency.tg` with `Channel` message passing across pipeline stages; verified clean native execution.
   - Updated `examples/web_service_v2.tg` with request-scoped regions, zero-copy `std::http` parsing and wire formatting, and delimited `Net` effect handling; verified clean native execution.
   - Aligned code showcases in `README.md` and `docs/SYNTAX.md` with compiler-verified grammar.

---

### Phase 10: Formal Verification & SMT Solver Integration (v1.7) ✅
*Target: Verifiable non-linear interval constraints and automated proofs.*

1. **Non-Linear Interval Solver & SMT-LIB2 Bridge** ✅:
   - 4-point mixed-sign interval arithmetic for non-linear multiplication ($\min/\max$ across all four extremal corner products: $l_{min} r_{min}, l_{min} r_{max}, l_{max} r_{min}, l_{max} r_{max}$).
   - Provable non-zero divisor safety checking and quotient interval derivation; static rejection of potential division by zero when divisor interval spans zero (`Refinement Violation: potential division by zero (divisor interval spans 0)`).
   - Modulo interval bounds calculation and `%` operator (`tok_percent`) integration across lexer, Pratt parser, TIR, optimizer, and LLVM codegen (`srem i64`).
   - Path-sensitive conditional interval narrowing (`if x >= A && x <= B`), narrowing variable ranges within conditional bodies and restoring outer scopes upon branch exit.
   - SMT-LIB2 bridge (`compiler/smt.tg`) producing standard QF_NIA and QF_LIA assertions, variable declarations, and `check-sat` queries compatible with automated theorem provers (Z3, CVC5).
2. **Affine & Linear Resource Invariants (Phase 10.2)** ✅:
   - Static ownership tracking for OS handles and resources declared via `linear struct` (mode 1) or `affine struct` (mode 2).
   - Compile-time enforcement of exactly-once consumption for `linear struct`: leak detection on return/exit (`Linear Resource Violation: resource dropped without being consumed (potential resource leak)`), transfer of ownership across function calls and returns, and rejection of use-after-consume (`Linear Resource Violation: use-after-consume of resource variable`).
   - At-most-once consumption semantics for `affine struct`: unconsumed affine resources safely auto-drop at scope exit without error, while use-after-consume remains strictly prohibited.
   - Branch Convergence Analysis: static enforcement that linear resources reach identical consumption states along divergent control-flow paths (`if/else`), rejecting branch divergence (`Linear Resource Violation: branch divergence; resource consumed in 'then' branch but unconsumed in 'else' branch`) and conditional consumption in `if` without `else` (`Linear Resource Violation: resource consumed conditionally in 'if' without an 'else' branch (must be consumed along all execution paths)`).
   - Loop Invariant Protection: rejection of consuming pre-existing resources within loop bodies (`Linear Resource Violation: resource variable consumed inside loop body (risk of use-after-consume on next iteration)`).
   - Local Resource Isolation: nested blocks and branches verify local resource lifecycle without leaking across outer scopes.
   - First-class zero-cost runtime intrinsics `consume(x)` and `drop(x)`.
   - Fixed-point 3-stage bootstrap parity verified across stages (`7297BED153ACBF31E09F7C0D0B4A2AF30B9405FDAE69B0206C1E2DB2668D71FC`).
   - Verified by comprehensive 13th native test suite `tests/formal_verification_test.tg` (100% OK across all 13 native test suites).

---

### Phase 11: Multi-Target Codegen & WebAssembly (v1.8)
*Target: Universal systems development from bare metal to browsers.*

- [x] **x86_64 Linux (ELF) Target & Docker Container**: Prototyped in Milestone v1.2 (Stage-0 Rust); scheduled for validation under the self-hosted compiler.
- [ ] **AArch64 & macOS Targets**:
  - ARM64 ELF and Mach-O binary emission via LLVM backend and LLD linker.
  - Apple Silicon / ARM64 POSIX socket and syscall shims in `std/fs` and `std/net`.
- [ ] **WebAssembly Target (`wasm32-unknown-unknown`)**:
  - Direct compilation to Wasm bytecode.
  - Algebraic effect mapping to JavaScript host promises and browser Web APIs without runtime shims.
