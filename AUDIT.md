# Tungsten Genesis (v1.3) Compiler Audit & Architectural Review

**Date:** September 2026  
**Scope:** `compiler/*.tg`, `std/*.tg`, `tests/*.tg`, `examples/*.tg`, `bin/`  
**Status:** Self-hosting confirmed for core imperative subset; severe architectural shortcuts and missing headline features documented.

---

## 1. Executive Summary

Tungsten achieved a notable milestone with Milestone v1.3 ("Genesis"): writing a self-compiling compiler in its own language (`compiler/*.tg`) and retiring the legacy Rust bootstrap compiler (`archive/stage0-rust/`) to produce bitwise fixed-point LLVM IR across bootstrap stages.

However, an audit of the compiler codebase reveals that the transition from the Stage-0 Rust prototype to the self-hosted Tungsten compiler left behind major gaps and introduced brittle shortcuts:

1. **The Headline Features Are Not Implemented:** Algebraic effects, user-defined refinement types, region escape analysis, and fiber-based structured concurrency are either discarded by the parser or entirely absent from `compiler/*.tg`.
2. **Critical Codegen Shortcuts Cause Silent Data Corruption:** Struct field offsets are resolved through a hardcoded global string table of compiler AST field names. Any user struct with custom field names maps every field to offset 0, overwriting memory. Array element stride (1 byte vs. 8 bytes) is decided by checking the string name of the variable.
3. **The Optimizer Pipeline Is Dead Code:** `forge.tg` lowers the AST to TIR and runs constant folding and DCE, but then passes the raw, unoptimized AST into `emit_llvm_ir()`.
4. **No Real Module System Exists:** Imports are discarded by the parser. Multi-file compilation is handled by a hardcoded file concatenator in `forge.tg` that only knows how to bundle the compiler itself and test files.
5. **Tooling Masks Failures:** `forge check` suppresses parser errors and prints zero errors with an exit code of 0 even when parsing fails on syntax errors.

---

## 2. What Is Actually Working

The following components are verified functional in the self-hosted toolchain:

* **Self-Hosting Bootstrap Loop:** `bin/tgc.exe` can compile `compiler/main.tg` into LLVM IR text, which links via `clang` into an executable capable of recompiling itself with bitwise identical LLVM IR output.
* **Bump Arena Memory Allocator:** Emitted directly into LLVM IR (`@tungsten_region_enter`, `@tungsten_region_alloc`, `@tungsten_region_grow`, `@tungsten_region_exit`) in `compiler/codegen.tg`. Supports 16 MB chunk chaining, 8-byte alignment masks, in-place contiguous growth, and bulk teardown.
* **Core Imperative Syntax:** Functions, scalar variables (`var`, `let`), while loops, if/else branching, recursion, primitive arithmetic, string literals, and explicit pointer casts.
* **Standard Library Collections:** `std/collections.tg` provides functional `Vec`, `StringBuffer`, and Robin Hood `HashMap` implementations using both system heap and scoped arena allocators.
* **C FFI & Windows System Calls:** `extern "C"` declarations reliably link and execute against MinGW CRT and Win32 libraries (`msvcrt`, `kernel32`, `gdi32`, `winmm`, `ws2_32`).

---

## 3. Detailed Audit Findings

### Finding 1: Struct Field Offsets Are Globally Hardcoded

**Location:** `compiler/codegen.tg`, lines 127–191 (`get_field_offset`)

The compiler does not compute field offsets from struct definitions. Instead, it uses a global string table matching compiler AST node fields:

```tungsten
fn get_field_offset(cg: &mut Codegen, sym: i64) -> i64 {
    let s = interner_lookup(interner_ref, sym);
    let p = s as *const u8;
    if unsafe { strcmp(p, "kind") == 0 } { return 0; }
    if unsafe { strcmp(p, "sym") == 0 }  { return 1; }
    if unsafe { strcmp(p, "int_val") == 0 } { return 2; }
    if unsafe { strcmp(p, "str_val") == 0 } { return 3; }
    // ... handles ~30 compiler-specific field names ...
    return 0; // Fallback for all other field names
}
```

**Impact:**  
Any struct containing field names not explicitly listed in this table assigns offset `0` to every field.

```tungsten
struct Account {
    balance: i64,
    account_id: i64,
}
// Both 'balance' and 'account_id' map to offset 0 (0 bytes).
// Setting account.account_id overwrites account.balance.
```

---

### Finding 2: Array Indexing Stride Depends on Variable Names

**Location:** `compiler/codegen.tg`, lines 193–221 (`is_byte_indexing`)

When indexing an array or pointer (`base[index]`), the compiler does not inspect the underlying element type. It inspects the identifier name of the variable:

```tungsten
fn is_byte_indexing(cg: &mut Codegen, left: *mut AstExpr) -> bool {
    // ...
    if left.kind == expr_var() {
        let s = interner_lookup(interner_ref, left.sym);
        let p = s as *const u8;
        if unsafe {
            strcmp(p, "old_keys") == 0 ||
            strcmp(p, "old_vals") == 0 ||
            strcmp(p, "old_occ") == 0 ||
            strcmp(p, "ptr_slot") == 0 ||
            strcmp(p, "stmt_slot") == 0
        } {
            return false; // Stride by 8 bytes (i64)
        }
        return true; // Default: Stride by 1 byte (i8)
    }
    true
}
```

**Impact:**  
Any user array of integers or pointers whose variable name is not `old_keys`, `old_vals`, `ptr_slot`, or `stmt_slot` is treated as a byte array (`i8*`). Indexing `arr[1]` strides by 1 byte instead of 8 bytes, causing unaligned memory corruption.

---

### Finding 3: TIR and Optimizer Passes Are Dead Code

**Location:** `compiler/forge.tg`, lines 204–209

```tungsten
forge_print("TGC: Lowering to TIR & optimizing...");
let tir_mod = lower_ast_program_to_tir(prog, comp_reg);
let opt_count = tir_optimize_module(tir_mod, comp_reg);

forge_print("TGC: Emitting LLVM IR...");
let mut ir_buf = emit_llvm_ir(prog, &mut interner, comp_reg);
```

**Impact:**  
`emit_llvm_ir()` receives `prog` (the AST), not `tir_mod`. All work done in `compiler/tir.tg` (SSA basic blocks) and `compiler/opt.tg` (constant folding, dead code elimination, bounds check elimination) is discarded immediately after execution. The emitted LLVM IR is generated directly from unoptimized AST nodes.

---

### Finding 4: Algebraic Effects Are Not Implemented

**Location:** `compiler/parser.tg`, lines 525–538; `compiler/lexer.tg`, lines 100–126

The lexer does not recognize `effect`, `handle`, `with`, or `resume`. In the parser, the `yields [...]` signature is treated as an optional annotation that is parsed in a loop and discarded:

```tungsten
if check(p, tok_ident()) {
    let s = interner_lookup(interner_ref, tok.sym);
    if unsafe { strcmp(s as *const u8, "yields") == 0 } {
        advance(p);
        expect(p, tok_lbracket(), "Expected '[' after yields");
        while !check(p, tok_rbracket()) && !check(p, tok_eof()) {
            advance(p); // Discards all effect names
        }
        expect(p, tok_rbracket(), "Expected ']' after effect list");
    }
}
```

**Impact:**  
All function calls compile to standard synchronous C function calls. `std/effects.tg` and `std/net.tg` fail to compile with dozens of syntax errors when parsed.

---

### Finding 5: User-Defined Refinement Types Cannot Be Declared

**Location:** `compiler/parser.tg`, lines 553–584; `compiler/typeck.tg`, lines 103–115

1. `parser.tg` has no rule for `type Name = ...;` at top level. Passing `type Health = u8[0..100];` fails with `Unexpected token at top level`.
2. When parsing type signatures (`parse_type`), intervals in brackets `[min..max]` are scanned and discarded.
3. The only refined types that exist are three hardcoded types registered in `typeck.tg`'s prelude:
   * `Percentage` (`u8[0..100]`)
   * `Port` (`i64[1..65535]`)
   * `Byte` (`u8[0..255]`)

---

### Finding 6: Region Escape Analysis Is Absent

**Location:** `compiler/typeck.tg`, line 307

The comment in `typeck.tg` reads:
```tungsten
// Type Checking Statements & Region Escape Analysis
```
Below this comment, `tc_stmt` checks variable assignment bounds for the three hardcoded prelude types, but performs no lifetime or escape tracking. A pointer allocated inside `region r { ... }` can be assigned to an outer variable or returned from a function without triggering any compiler warning or error, causing dangling pointer access after `@tungsten_region_exit`.

---

### Finding 7: No Module System; Monolithic Hardcoded Bundler

**Location:** `compiler/parser.tg`, lines 558–563; `compiler/forge.tg`, lines 60–179

1. In `parser.tg`, `import <anything>;` consumes tokens until `;` and performs no file loading or name binding.
2. In `forge.tg`, `load_source_bundle` hardcodes the concatenation of 15 specific compiler source files in order.
3. If compiling any file outside `compiler/` or `tests/`, `load_source_bundle` reads only that single file. `import std.collections;` in user code does nothing, leaving collections undefined.

---

### Finding 8: `forge check` Masks Syntax Errors

**Location:** `compiler/forge.tg`, lines 260–280

```tungsten
pub fn forge_run_check(path: String, arena: *mut u8) -> i64 yields [FS, Diagnostics, IO] {
    // ...
    let tokens = tokenize(src, &mut interner, arena);
    let prog = parse_program(tokens, &mut interner, arena);
    let tc_ok = typecheck_program(prog, &mut interner, arena);
    if !tc_ok {
        report_err("forge check failed: refinement or type violations detected", 1, 1);
        return 1;
    }
    forge_print("forge: check passed: 0 errors, refinement invariants statically proven");
    0
}
```

**Impact:**  
`parse_program` logs syntax errors via `report_err` but does not set any error flag on `prog`. If the typechecker finds no errors among whatever AST nodes were salvaged, `forge check` prints `0 errors, refinement invariants statically proven` and exits with `0`, despite syntax errors printed to stdout.

---

### Finding 9: Flagship Documentation Examples Do Not Compile

Both flagship examples displayed in `README.md` fail compilation immediately when tested:

* `examples/player.tg`: **72 syntax and type errors** (`type`, `?`, `handle`, `with`, `!` are unparsed).
* `examples/web_service_v2.tg`: **35 syntax errors** (`nursery`, `println!` macro syntax are unparsed).
* `std/net.tg`: **78 syntax errors** (`effect Net` is unparsed).

---

### Finding 10: Undocumented External Clang / CRT Dependencies

**Location:** `compiler/forge.tg`, lines 220–235

Compilation of executables calls `clang` via `system()`:
```text
clang --target=x86_64-pc-windows-gnu -nostartfiles -o <out> target/crt/crt2.o -Ltarget/crt -lmingw32 -lmingwex -lmsvcrt -lkernel32 ...
```
Because `target/` is gitignored, a fresh clone on a machine without `clang` or `target/crt/crt2.o` cannot build binaries.

---

## 4. Remediation Plan

### Phase A: Compiler Foundation & Correctness (Milestone v1.4)
* [x] **Dynamic Struct Layout:** In `compiler/typeck.tg`, computed struct field offsets based on field declaration order and size. Replaced the `get_field_offset` string comparison table in `codegen.tg` with dynamic symbol offset resolution.
* [x] **Type-Directed Indexing:** Emits GEP stride based on the base pointer's resolved type rather than variable name checks.
* [x] **Connect TIR Pipeline:** Changed `emit_llvm_ir` to lower directly from `TirModule` instead of `AstProgram`, activating constant folding, DCE, and bounds elimination with 3-stage bootstrap fixed-point parity.
* [x] **Real Module Resolution:** Replaced `load_source_bundle` with an AST-level import resolver that traverses `import <path>;` and loads referenced `.tg` files into the compilation unit.
* [x] **Strict Error Propagation:** `forge check`, `forge build`, and `tgc` fail immediately with exit code 1 if lexer, parser, or typechecker report errors.
* [x] **Real `forge clean`:** Implemented filesystem removal of build artifacts in `target/`.
* [x] **Vendor CRT / Toolchain Prerequisites:** Moved `crt2.o`, `dllcrt2.o`, and MinGW static libraries into committed `lib/crt/` tree, eliminating dependencies on uncommitted `target/crt/`.

### Phase B: Delivering Stated Language Invariants (Milestone v1.6)
* [x] **Refinement Grammar & Solver:** Support `type Name = Primitive[min..max];` in `parser.tg`, storing bounds in the type environment (Phase 9.1).
* [x] **Region Escape Analysis:** Track region lifetimes in `typeck.tg` and reject references that outlive their declaring region (Phase 9.2).
* [x] **Parametric Generics with Call-Site Monomorphization:** Replaced token-skipping in the parser with AST generic parameters (`AstStruct`, `AstFn`), turbofish call-site syntax (`::<T>`), pre-pass deduplicated monomorphization (`ident__concrete`), and compile-time invariant enforcement prohibiting unresolved generic types at code generation (Phase 9.3).
* [ ] **Algebraic Effects & Concurrency:** Design a realistic lowering path for effects (via delimited continuations or a native C runtime fiber scheduler).
* [ ] **Example & Docs Alignment:** Update `examples/` and documentation to accurately reflect supported syntax and semantics.
