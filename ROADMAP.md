# Tungsten Language Roadmap

> **Vision:** A systems-level programming language that provides the fearless concurrency and zero-cost abstractions of Rust, while eliminating explicit lifetime annotations through **Region-Based Memory Management**, eliminating the async/await divide through **Algebraic Effects**, and eliminating runtime bounds panics through **Refinement Types**.

---

## Progress Overview

| Phase | Milestone | Focus Area | Status |
|:---|:---|:---|:---:|
| **v0.1** | **Working Prototype** | Parser, Typechecker, Refinement Intervals, Effect VM, CLI | **Completed** |
| **v0.2** | **Developer Experience** | Standard Library (`std`), Formatter (`forge fmt`), Language Server (`tungsten-lsp`) | **Completed** |
| **v0.3** | **Advanced Type System** | Relational Refinements, Generics, Effect Polymorphism | **Completed** |
| **v0.4** | **Tungsten IR & Optimization** | SSA / CFG Intermediate Representation (TIR), Continuation Lowering | **Completed** |
| **v0.5** | **Native Codegen** | Cranelift (Fast JIT/Debug) & Host Runtime ABI Code Generation | **Completed** |
| **v0.6** | **Colorless Concurrency** | M:N Work-Stealing Fiber Scheduler via Algebraic Effects | **Completed** |
| **v0.7** | **Physical Region Allocator** | Machine-level Arena Scopes & $\mathcal{O}(1)$ Region Teardown | **Completed** |
| **v1.0** | **Production & Ecosystem** | `Forge.lock` Package Manager, Stdlib Expansion, Self-Hosting | **Next Priority** |


---

## Completed Milestones

### v0.1: Working Prototype
- [x] **Frontend (`tungsten-syntax`)**: Tokenizer and recursive-descent parser handling refinement syntax (`u8(0..=100)`), effect signatures (`yields [Db, IOError]`), and delimited effect handlers (`handle { ... } with Effect { ... }`).
- [x] **Semantic Analysis (`tungsten-typeck`)**:
  - Constant interval constraint arithmetic (`[min, max]`).
  - Statically proven bounds checking on assignments and arithmetic.
  - Compile-time effect row checking and unhandled effect rejection.
- [x] **Runtime Evaluator (`tungsten-vm`)**:
  - Delimited continuation effect dispatcher supporting mock and async handlers.
  - Scoped reference borrowing (`&mut`) without explicit lifetime annotations.
- [x] **Tooling (`forge`)**: CLI runner supporting `forge check` and `forge run`.

### v0.2: Developer Experience (DX) & Tooling
- [x] **Standard Library (`std/`)**:
  - Pre-registered prelude refinement types: `Percentage`, `Port`, `NonZeroU32`, `Byte`.
  - Core algebraic effects: `IO`, `Random`, `State`, `Time`, `Yield`.
- [x] **Opinionated Formatter (`forge fmt`)**:
  - AST-aware pretty printer with canonical 4-space indentation, operator spacing, and trailing commas.
  - CI verification support via `forge fmt --check`.
  - Proved idempotent: $\mathrm{fmt}(\mathrm{fmt}(x)) = \mathrm{fmt}(x)$.
- [x] **Language Server Protocol (`tungsten-lsp`)**:
  - JSON-RPC 2.0 stdio server (`forge lsp`) for VS Code, Neovim, and Helix.
  - Real-time diagnostics with exact source spans on document open and edit.
  - Hover tooltips detailing inferred types, refinement intervals, and effect rows.
  - Editor "Format on Save" provider.

### v0.3: Advanced Type System & Frontier PL
- [x] **Relational Refinement Types**:
  - Express inter-parameter inequalities: `fn subslice_len(start: usize, end: usize(>= start)) -> usize`.
  - Linear arithmetic verification between function parameters, return values, and struct fields.
  - Statically proven bounds checking and compile-time rejection of relational contract violations.
- [x] **Generics & Parametric Polymorphism**:
  - Parametric structs: `struct Container<T> { value: T }`.
  - Generic functions: `fn wrap<T>(item: T) -> Container<T>`.
  - Bidirectional Hindley-Milner-style unification and type substitution (`Subst`, `unify`, `substitute`).
- [x] **Effect Polymorphism & Higher-Order Functions**:
  - First-class function types: `fn apply<T, U, E>(val: T, f: fn(T) yields [E] -> U) -> U yields [E]`.
  - Caller-agnostic higher-order abstraction over arbitrary effect rows without color segregation.
  - Transparent execution across both pure and effectful handlers in `tungsten-vm`.

### v0.4: Intermediate Representation (TIR) & Optimization Passes
- [x] **SSA Intermediate Representation (`tungsten-tir`)**:
  - Linearized Basic-Block Control Flow Graph (CFG) preserving types, refinement intervals, and effect signatures.
  - Verification pass (`verify::verify_module`) ensuring CFG integrity, block reachability, and valid terminators.
- [x] **AST to TIR Lowering**:
  - Lowers expressions, loops, branching, and function calls into SSA instructions and explicit branch jumps.
  - Delimited algebraic effect lowering (`handle { ... } with Effect { ... }`) into `HandleEffect` frames and `Resume` continuations.
- [x] **Middle-End Optimization Pipeline**:
  - Constant folding and propagation pass (`const_fold`).
  - Dead code and unreachable block elimination (`dce`).
  - Redundant refinement bounds check elimination (`bounds_elim`) proving zero-cost abstraction for safe intervals.
- [x] **CLI Tooling**:
  - `forge tir [--opt] <file.tg>` for inspecting unoptimized and optimized intermediate representation.

### v0.5: Native Codegen via Cranelift & JIT Execution
- [x] **Machine Codegen Backend (`tungsten-codegen`)**:
  - Direct translation of TIR basic blocks into Cranelift IR and native machine assembly (x86_64 / AArch64).
  - Native ABI and runtime bridge:
    - Zero-overhead C-ABI host imports: `tungsten_print_i64`, `tungsten_println_i64`, `tungsten_print_str`, `tungsten_println_str`, `tungsten_io_print`.
    - Native refinement trap & panic handler (`tungsten_refinement_panic`).
    - Heap memory allocation bridge (`tungsten_alloc`).
  - Native string constant pools and null-terminated string representation.
  - First-class function pointers and indirect native calls (`call_indirect`).
  - Dynamic struct allocation and offset-based field access.
- [x] **In-Memory JIT Compilation Engine**:
  - `JitEngine` compiling TIR modules into executable machine code in memory.
  - Verification across effectful database records, higher-order functions, and generic containers.
- [x] **CLI Tooling**:
  - `forge run --native <file.tg>` for direct native JIT execution.
  - `forge build <file.tg>` subcommand for native compilation.

### v0.6: Colorless Concurrency via Algebraic Effects & M:N Work-Stealing Fiber Runtime
- [x] **Fiber Engine & Structured Concurrency (`tungsten-fiber`)**:
  - Multi-threaded M:N work-stealing scheduler (`Scheduler`) distributing fibers across worker threads with local queues and global injector queue.
  - Structured concurrency nursery scopes (`Nursery<T>`) guaranteeing lexical lifecycle boundaries and deterministic fiber joins.
  - Cross-thread unbounded and bounded message channels (`Channel<T>`) with thread-safe send, recv, and try_recv.
- [x] **Concurrency Algebraic Effects in Typechecker (`tungsten-typeck`)**:
  - Registered `Async`, `Channel`, and `FiberHandle` in core type system.
  - Type inference and effect validation for `Async::spawn`, `Async::yield_now`, `Async::await_fiber`, `Async::sleep`, `Channel::new`, `Channel::send`, and `Channel::recv`.
- [x] **Thread-Safe Runtime Execution (`tungsten-vm`)**:
  - Converted heap reference values to thread-safe `Arc<Mutex<Value>>`.
  - Effect dispatch integration routing `Async` and `Channel` operations through multi-threaded fiber scheduler.
- [x] **Native Codegen & Cranelift JIT Concurrency (`tungsten-codegen`)**:
  - C-ABI runtime bindings for fiber spawning, yielding, sleep, and channel message passing.
  - Cranelift IR lowering for `Async` and `Channel` algebraic effect operations into direct native host calls.
  - Tested and verified end-to-end via `examples/fibers_and_concurrency.tg` on both VM and native JIT (`--native`).

### v0.7: Physical Region Allocator & Compile-Time Region Inference
- [x] **Region Syntax & Grammar (`tungsten-syntax`)**:
  - `region [name] { ... }` lexical block expression syntax, parser, and AST node `ExprKind::Region`.
  - Full formatter (`forge fmt`) support and round-trip verification.
- [x] **Compile-Time Region Inference & Escape Analysis (`tungsten-typeck`)**:
  - Region IDs attached to references (`Type::Ref { is_mut, inner, region: Option<RegionId> }`).
  - Automatic inferencing of region scopes without explicit `<'a>` lifetime annotations.
  - Strict Linear Escape Analysis rejecting references escaping local regions via returns or outer variable assignments.
- [x] **TIR Region Lifecycle Instructions (`tungsten-tir`)**:
  - `Instruction::RegionEnter` and `Instruction::RegionExit` instruction variants.
  - Arena-directed `RValue::StructInit { name, fields, arena: Option<Operand> }`.
  - Region-aware Dead Code Elimination (`dce`) and basic block lowering.
- [x] **Native Machine Region Allocator (`tungsten-codegen`)**:
  - Zero-overhead native bump allocator `PhysicalArena` with $\mathcal{O}(1)$ bulk teardown.
  - Runtime C-ABI symbols: `tungsten_region_enter`, `tungsten_region_alloc`, `tungsten_region_exit`.
  - Cranelift JIT translation targeting physical bump arenas inside region blocks.
- [x] **Full Dual-Backend Execution (`tungsten-vm` & `tungsten-codegen`)**:
  - Validated end-to-end with `examples/regions_and_lifetimes.tg` across tree-walking VM and native Cranelift JIT.

---


## Upcoming Milestones

### Phase 1: Advanced Type System & Frontier PL (v0.3)
*Target: Elevate Tungsten's typechecker from constant intervals to relational mathematical proofs and full polymorphism.*

1. **Relational Refinement Types**:
   - Express inter-variable constraints:
     ```rust
     fn slice(arr: Array, start: usize, end: usize(>= start && <= arr.len)) -> Slice
     ```
   - Linear arithmetic verification between function parameters, return values, and struct fields.
2. **Generics & Parametric Polymorphism**:
   - Generic structs: `struct Box<T> { value: T }`, `struct Result<T, E>`.
   - Generic functions: `fn identity<T>(val: T) -> T`.
3. **Effect Polymorphism**:
   - Higher-order functions that abstract over the effects of caller closures:
     ```rust
     fn map<T, U, E>(arr: [T], f: fn(T) yields E -> U) -> [U] yields E
     ```
   - Solves the classic problem where higher-order functions in Rust cannot cleanly handle both sync, async, and fallible closures.
4. **SMT Solver Integration**:
   - Optional Z3 solver bridge to verify complex non-linear arithmetic constraints and inductive properties when interval arithmetic is insufficient.

---

### Phase 2: Compiler Backend & Optimization (v0.4 & v0.5)
*Target: Transition from tree-walking evaluation to optimized native machine code.*

1. **Tungsten Intermediate Representation (TIR)**:
   - High-level Static Single Assignment (SSA) form preserving type refinements and effect operations.
   - Middle-end optimization passes:
     - Dead Code Elimination (DCE).
     - Constant Folding & Propagation.
     - Redundant Refinement Bounds Check Elimination (proven zero-cost).
     - Inline Expansion.
2. **Continuation Lowering**:
   - Transform delimited effect handlers into zero-cost stack-allocated state machines for synchronous execution.
   - Transform yielding points into fiber yield frames for asynchronous execution.
3. **Native Code Generators (`tungsten-codegen`)**:
   - **Cranelift Backend**: Blazing-fast compilation times for local iterative builds and debug mode (`forge build`).
   - **LLVM Backend** (via `inkwell`): World-class optimizations, vectorization, and link-time optimization (LTO) for release binaries (`forge build --release`).
   - **WebAssembly Target (`wasm32-unknown-unknown`)**: Compile Tungsten directly to the browser, leveraging algebraic effects to map I/O directly into JavaScript host promises.

---

### Phase 3: Fearless Concurrency via Algebraic Effects (v0.6)
*Target: Zero-color, high-performance concurrency multiplexed over OS threads.*

1. **Green-Thread / Fiber Engine**:
   - Stackful coroutines / delimited continuations driven by an `Async` / `Spawn` effect handler.
   - Functions never need `async fn` or `.await` syntax: calling an I/O operation naturally yields execution back to the fiber scheduler.
2. **M:N Work-Stealing Runtime**:
   - Multi-threaded scheduler multiplexing $M$ Tungsten fibers across $N$ physical CPU cores.
   - Non-blocking I/O event loop driver (epoll / kqueue / IOCP).
3. **Structured Concurrency**:
   - Nursery scopes ensuring all spawned concurrent tasks must join or abort before their enclosing lexical scope finishes.

---

### Phase 4: Native Region-Based Memory Management (v0.7)
*Target: Complete elimination of explicit lifetime syntax `<'a>` at machine level.*

1. **Compile-Time Region Inference**:
   - Tofte-Talpin region inference augmented with linear and affine ownership types.
   - Automatically partitions heap allocations into lexical arena frames.
2. **$\mathcal{O}(1)$ Bulk Deallocation**:
   - Arena scopes freed in single pointer bumps without traversing individual heap objects (except for registered foreign resource finalizers).
3. **Linear Escape Analysis**:
   - The compiler guarantees pointers cannot outlive their enclosing region scope, guaranteeing 100% spatial and temporal memory safety without garbage collection.

---

### Phase 5: Ecosystem & Production Hardening (v1.0)
*Target: A rock-solid, production-ready language and package ecosystem.*

1. **Dependency Management in `forge`**:
   - Full `Forge.toml` specification: dependencies, git repositories, targets, and compiler profiles.
   - Deterministic `Forge.lock` lockfile resolution.
2. **Expanded Standard Library**:
   - `std::collections` (Vector, HashMap, BTreeMap).
   - `std::net` (TCP, UDP, TLS abstractions over effects).
   - `std::sync` (Channels, Mutexes, Atomic operations).
3. **Language Specification & Formal Verification**:
   - Formal EBNF grammar and operational semantics.
   - Test suites covering soundness, progress, and preservation proofs.
4. **Self-Hosting Toolchain**:
   - Rewriting the Tungsten compiler frontend in Tungsten itself.

---

## Suggested Next Immediate Sprint: Phase 5 (v1.0)

To complete the journey toward a production-grade systems language, the recommended immediate next sprint is **Phase 5: Ecosystem & Production Hardening (v1.0)**:
1. **Dependency Management in `forge`**: Deterministic `Forge.lock` lockfile resolution and package registry integration.
2. **Standard Library Expansion**: Collections (`Vector`, `HashMap`), networking (`std::net`), and concurrency primitives (`std::sync`).
3. **Formal Specification & Self-Hosting**: EBNF language grammar and bootstrapping the compiler frontend in Tungsten.

