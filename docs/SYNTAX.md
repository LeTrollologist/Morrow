# Morrow Language & Syntax Reference (v2.0.1)

> This document describes **what the compiler actually accepts and generates**, not
> an aspirational design. Every construct below was compiled (and where relevant
> executed) against `forge 2.0.1 (morrow standalone 2026)`; constructs that are
> specified elsewhere but *not* implemented are collected in
> [§14 Not Implemented / Known Limitations](#14-not-implemented--known-limitations).
>
> Re-verify any claim with `forge check <file.mw>` for typecheck-only or
> `forge build <file.mw> -o out.exe` for the full pipeline (Morrow → TIR → LLVM IR →
> clang). Calls are now checked for callee existence and arity (v2.0.2), so a
> typo or a wrong argument count is a type error rather than a wrong program.

---

## 1. Lexical Grammar

### Comments
```morrow
// Single-line comment to end of line

/* multi-line block comment
   (newlines are counted, so diagnostics keep correct line numbers) */
```
Both forms are supported.

### Identifiers
Start with an ASCII letter or `_`, followed by letters, digits or `_`:
```morrow
var count = 0;
let user_id = 42;
let _scratch = "temp";
```

### Literals
| Literal | Supported | Notes |
|---|:---:|---|
| Decimal integer `42` | ✅ | Underscore separators allowed: `100_000` |
| Hexadecimal `0xFF`, `0x10` | ✅ | `0x` / `0X` prefix |
| Negative integer `-1` | ✅ | Unary minus on a literal |
| Boolean `true` / `false` | ✅ | |
| String `"text"` | ✅ | See escapes below |
| `null` | ✅ | The null pointer constant (`0`); usable where a pointer type is expected |
| Float `3.14` | ❌ | `.` is field access; parse error |

String escapes accepted by the lexer: `\n`, `\r`, `\t`, `\0`, `\\`, `\"`.
Strings are NUL-terminated in generated code.

### Keywords recognised by the lexer
```text
fn      let     mut     var     if      else    while   loop    break
continue return struct  type    effect  perform handle  with    resume
region  extern  unsafe  import  pub     as      true    false   null
```
Contextual words handled by the parser but **not** lexed as keywords:
`yields`, `linear`, `affine`, `const`.
Intrinsics (never declared, resolved by the code generator):
`println`, `consume`, `drop`.

Words that appear in older drafts but are **not** supported at all:
`for`, `in`, `match`, `enum`, `nursery`.

---

## 2. Type System & Refinements

Morrow is statically typed with bidirectional inference and a compile-time
integer-interval solver. There are no floating-point types in the implemented
subset.

### Primitive types
| Type | Supported | Notes |
|---|:---:|---|
| `i8`, `i16`, `i32`, `i64` | ✅ | Signed integers (`i64` is the default arithmetic width) |
| `u8`, `u16`, `u32`, `u64` | ✅ | Unsigned integers |
| `bool` | ✅ | `true` / `false` |
| `String` | ✅ | NUL-terminated byte string, heap or region allocated |
| `void` | ✅ | Accepted as a return type; functions may also omit the return type |
| `f32`, `f64` | ❌ | Parsed as a type name but unusable — there are no float literals |

Verified: `let a: i8 = 1; let b: u16 = 2; let c: u64 = 3;` compiles and adds.

### References and pointers
```morrow
&T          // immutable borrow
&mut T      // mutable borrow
*mut T      // raw mutable pointer (C ABI)
*const T    // raw const pointer (C ABI)
```

### Refinement types
Refinements attach an interval to an integer type:
```morrow
type Percentage    = u8[0..100];
type Port          = u16[1..65535];
type HttpStatusCode = u16[100..599];
type Span          = i64[-5..5];      // negative bounds are allowed

var ok: Port = 8080 as Port;   // accepted
var bad: Port = 70000 as Port; // rejected: "Refinement Constraint Violation: value out of bounds"
```
What the checker actually proves today:
- **Literal casts** are range-checked against the interval (the `70000` case above).
- **Division by a refined divisor whose interval spans zero is rejected**:
  `x / d` where `d: i64[-5..5]` → "Refinement Violation: potential division by zero
  (divisor interval spans 0)".
- **Casts from variables are not range-checked**, and there is **no path-sensitive
  narrowing**. Both of the following compile today:
  ```morrow
  fn to_pct(x: i64) -> Percentage { x as Percentage }   // accepted
  fn f(x: i64) -> i64 { let v: i64 = 200; let p = v as Percentage; 0 }  // accepted
  ```
  Treat refinements as checked documentation plus a literal/divisor guard, not a
  full proof system.

### Parametric generics (call-site monomorphization)
Generics are monomorphized ahead of codegen and require explicit turbofish at use
sites:
```morrow
struct Pair<T> {
    first: T,
    second: T,
}

fn identity<T>(item: T) -> T {
    item
}

fn main() -> i64 {
    let a = identity::<i64>(42);
    let p = Pair::<i64> { first: 10, second: 20 };
    a + p.first      // 52
}
```
Verified: compiles and returns `52`. A generic function is expected to have
exactly one type parameter; deeper parameter lists are not supported.

---

## 3. Linear & Affine Resource Types

### `linear struct` — exactly-once consumption
```morrow
linear struct FileHandle {
    fd: i64,
}

fn close_file(f: FileHandle) {
    consume(f);
}

fn main() -> i64 {
    let f = FileHandle { fd: 1 };
    close_file(f);
    0
}
```
Falling out of scope without `consume` is rejected:
```
[ERROR] Linear Resource Violation: resource dropped without being consumed
```
Branch convergence is enforced — consuming on one path only is rejected:
```
fn branch_check(cond: bool, f: FileHandle) {
    if cond { close_file(f); } else { close_file(f); }   // accepted
}
```

### `affine struct` — at-most-once consumption
```morrow
affine struct TempBuffer { ptr: *mut u8 }
```
`affine struct` parses and `consume` works, but **the compiler still requires
consumption on every path**. An early `return` before `consume` is rejected:
```
[ERROR] Linear Resource Violation: resource consumed conditionally in 'if'
        without an 'else' branch (must be consumed along all execution paths)
```
Automatic drop-on-early-return is therefore **not** implemented in v2.0.1,
despite older drafts claiming it.

### Resource intrinsics
- `consume(res)` — marks a resource consumed; later use is a `use-after-consume`
  violation.
- `drop(res)` — accepted as an intrinsic name alongside `consume`.


---

## 4. Memory Model: Scoped Regions

A region is a bump arena with bulk teardown. It can be used as a statement or as
an expression (its tail value is the block value):

```morrow
fn build() -> i64 {
    region r {
        7
    }
}

fn main() -> i64 {
    build()      // returns 7
}
```
Verified both as a statement and as an rvalue (`let x = region r { ... };`).

### Region-aware allocation
Allocators take the region handle as their first argument:
```morrow
region r {
    let mut v = vec_new_in(r);                                   // Vec<T>
    let mut b = string_buffer_new_in(r);                         // StringBuffer
    let mut m = hashmap_new_in(r, 256);                          // HashMap (capacity hint)
    let raw = unsafe { morrow_region_alloc(r, 64, 8) };          // raw arena bytes
}
```
Heap variants exist without the `_in` suffix (`vec_new()`, `string_buffer_new()`,
`hashmap_new()`), as do raw `morrow_alloc(size, align)` / `free(ptr)`. Note that
`hashmap_new()` takes no arguments while `hashmap_new_in(arena, initial_cap)` takes
two — the compiler now enforces both.

### Escape analysis is not enforced
References into a region are **not** currently rejected when they escape:
```morrow
fn illegal_escape() -> &String {
    region r {
        let mut s = string_buffer_new_in(r);
        string_buffer_push_str(&mut s, "hello");
        &s
    }
}
```
This compiles; the failure surfaces later (undefined symbol at link, or
use-after-free at runtime). Treat the "linear escape analysis" claim as roadmap,
not behaviour, in v2.0.1.

---

## 5. Colorless Algebraic Effects

### Effect declarations and `yields`
```morrow
effect Database {
    fn query(sql: String) -> String;
    fn execute(sql: String) -> i64;
}

effect Logger {
    fn log(msg: String) -> i64;
}

fn find_user(id: i64) -> String yields [Database, Logger] {
    perform Logger.log("Searching database...");
    perform Database.query("SELECT name FROM users WHERE id = 1")
}
```
A call to an effectful function from a function that does not declare (or handle)
the effect is rejected: "Unhandled effect in function call; must be declared in
yields contract or handled".

### Delimited handlers
The **only** accepted arm syntax is a function taking the continuation first:
```morrow
fn main() -> i64 {
    let name = handle {
        handle {
            find_user(101)
        } with Logger {
            fn log(k, msg: String) {
                resume(k, 0);
            }
        }
    } with Database {
        fn query(k, sql: String) {
            resume(k, "Alice");
        }
        fn execute(k, sql: String) {
            resume(k, 1);
        }
    };
    if name == "Alice" { return 0; }
    1
}
```
Verified: compiles and returns 0 (nested handlers, `perform` values flowing back).

Rules that follow from the desugaring:
- Every operation of the effect must have an arm. A partial handler is rejected:
  "Handler does not implement all operations of effect".
- Arms are **desugared into top-level functions**, so they cannot capture locals
  from the enclosing function. Communicate through `resume(k, value)` and the
  value of the `handle` expression.
- An arm that never calls `resume` is abortive; the handle expression evaluates to
  the arm's trailing expression. This is how `tests/net_test.mw` observes
  argument plumbing.
- Shorthand `op(args) => resume(...)` arms (shown in older drafts) do **not**
  parse.


---

## 6. Concurrency

**Structured concurrency is not implemented.** There is no `nursery` keyword, no
`n.spawn`, and no fiber/IOCP task pool in the v2.0.1 compiler (`nursery` is a parse
error).

Channels exist in the standard library as an *effect*, with handler-provided
semantics:
```morrow
import std::sync;
// effect Channel { fn send(handle: i64, msg: i64) -> i64; fn recv(handle: i64) -> i64; }
// channel_open<T>()  -> ChannelHandle<T>   (no capacity argument)
// channel_send_msg<T>(&ch, value)
// channel_recv_msg<T>(&ch)
```
They require a `handle { ... } with Channel { fn send(k, h, m) { ... } fn recv(k, h) { ... } }`
and a single generic argument, e.g. `channel_open::<i64>()`. Older drafts showing
`channel_open(10)` are wrong: the function takes no capacity parameter and the
arity is not checked, so the extra argument is silently ignored.

---

## 7. Declarations & Items

### Functions
```morrow
fn add(a: i64, b: i64) -> i64 {
    a + b            // trailing expression is the return value
}

fn procedure(flag: bool) -> void {
    if flag {
        println("true");
        return;
    }
    println("false");
}
```
Both `return value;` and a trailing expression work. A missing return type means
"returns nothing".

### Structs
```morrow
struct User {
    id: i64,
    name: String,
    is_admin: bool,
}

fn make() -> User {
    User { id: 1, name: "Morrow Admin", is_admin: true }
}

fn read_id(u: &User) -> i64 {
    u.id
}
```
Struct literals, field access, `&T` / `&mut T` parameters and struct return values
are all supported (the stdlib uses them heavily).

### Enums and `match`
**Not implemented.** `enum` and `match` are parse errors. The shipped example
`examples/enums_and_arrays.mw` demonstrates the supported workaround: a struct
with an integer tag plus constants.

### Type aliases
```morrow
type UserId = i64;
type Port = u16[1..65535];
```
Aliases accept both plain types and refinement intervals.

### Imports
Both separators are accepted and resolve to the same module file:
```morrow
import std.io;
import std::io;
import compiler::diagnostics;
```

---

## 8. Statements & Control Flow

```morrow
let immutable_x = 10;      // immutable binding
var counter = 0;           // mutable binding
let mut total: i64 = 0;    // mutable with explicit type
counter = counter + 1;     // assignment

if x > 100 {
    println("large");
} else if x > 50 {
    println("medium");
} else {
    println("small");
}

var i = 0;
while i < 10 {
    i = i + 1;
}

loop {
    i = i + 1;
    if i > 3 { break; }
}
```
Supported: `let`, `var`, `let mut`, assignment, `if`/`else if`/`else`, `while`,
`loop`, `break`, `continue`, `return`, nested blocks.

Not supported: `if` **as an expression** (`let x = if c { 1 } else { 2 };` — parse
error), closures (`let f = |x| ...`), `for`, `in`, `match`.

---

## 9. FFI & Unsafe

```morrow
extern "C" {
    fn malloc(size: i64) -> *mut u8;
    fn free(ptr: *mut u8) -> void;

---

## 10. Package Management & Locking

### Manifest (`Forge.toml`)
```toml
[package]
name = "app"
version = "0.1.0"
edition = "2026"

[dependencies]
math_lib = { path = "../math_lib", version = "^1.2.0" }
```
`forge new <name>` scaffolds this file plus `src/`.

### SemVer subset
- Exact: `1.2.3` or `=1.2.3`
- Caret: `^1.2.3` → `[1.2.3, 2.0.0)`; `^0.2.3` → `[0.2.3, 0.3.0)`; `^0.0.3` → `[0.0.3, 0.0.4)`
- Wildcards: `*`, `1.*`, `1.2.*`

Covered by `tests/package_tests.mw` and `tests/package_negative_test.mw` (both pass).

### `Forge.lock` (actual format)
`forge resolve` writes a canonical, alphabetically sorted lockfile. Empty
dependency lists are omitted:
```toml
# This file is automatically generated by Forge. Do not edit.
version = 1

[[package]]
name = "algebra_lib"
version = "0.8.1"
source = "local"
path = "../algebra_lib"

[[package]]
name = "math_lib"
version = "1.2.4"
source = "local"
path = "../math_lib"
dependencies = [
    "algebra_lib 0.8.1",
]
```
Invariant: every entry listed under `dependencies` must exist as a top-level
`[[package]]` with that exact version. `forge lock` re-verifies the closure.

---

## 11. Multi-Target Compilation

`--target <triple>` is honoured and the produced artifacts were verified by magic
bytes:

| Target triple | Artifact (verified) |
|---|---|
| `x86_64-pc-windows-gnu` (default) | Windows PE `.exe` |
| `x86_64-unknown-linux-gnu` | ELF 64-bit (`7F 45 4C 46`) |
| `aarch64-unknown-linux-gnu` | ELF 64-bit object (`.o`) |
| `wasm32-unknown-unknown` | WebAssembly (`00 61 73 6D`) |

```powershell
mwc examples\bootstrap_sample.mw --target wasm32-unknown-unknown -o target\app.wasm
```

Requirements and caveats:
- **clang must be on `PATH`.** Morrow emits LLVM IR; clang links it. Windows host
  linking also needs the MinGW CRT objects shipped in `lib/crt`
  (`crt2.o`, `libmingw32.a`, `libmingwex.a`, `libmsvcrt.a`, …).
- Cross-linking Linux/AArch64 from Windows requires a matching sysroot; when
  linking fails the compiler falls back to emitting a relocatable `.o`.
- There is **no CodeView/PDB debug information** and no LTO/`-ffunction-sections`
  pass despite older roadmap text — the emitted IR contains no debug metadata and
  the link line uses only `-O2`-style flags.

    fn strlen(s: *const u8) -> usize;
}

fn main() -> i64 {
    unsafe {
        let p = malloc(64);
        p[0] = 1 as u8;
        free(p);
    }
    0
}
```
- `extern "C"` blocks may appear at any module level; declarations are emitted
  once per module.
- Pointer dereference and indexing must sit inside `unsafe { ... }`.
- Calling an extern function does **not** require `unsafe` (the stdlib and tests
  call `puts`/`strlen` freely).
- **Arity and argument types**: callee existence and argument **count** are
  validated (v2.0.2) — `hashmap_new(16)` and `add(1)` are type errors. Argument
  *types* are still not compared, so passing a `String` where an `i64` is declared
  is accepted.


---

## 12. Standard Library Reference (verified signatures)

Names below are the real declarations in `std/*.mw`. Since v2.0.2 the compiler
checks the callee's existence and the argument count, so these signatures are
enforced at every call site (argument *types* are still not compared).

### `std.collections`
```morrow
hashmap_new() -> HashMap                 // no capacity argument
hashmap_new_in(arena: *mut u8, initial_cap: i64) -> HashMap
hashmap_insert(map: &mut HashMap, key: String, val: i64)
hashmap_get(map: &HashMap, key: String) -> i64
hashmap_len(map: &HashMap) -> i64

vec_new() -> Vec<T>          vec_new_in(arena) -> Vec<T>
vec_push(&mut Vec<T>, item)  vec_pop(&mut Vec<T>) -> T
vec_get(&Vec<T>, i) -> T     vec_set(&mut Vec<T>, i, value)
vec_len(&Vec<T>) -> i64      vec_cap(&Vec<T>) -> i64

string_buffer_new() -> StringBuffer      string_buffer_new_in(arena) -> StringBuffer
string_buffer_push_str(&mut StringBuffer, text: *const u8, text_len: i64)
string_buffer_as_ptr(&StringBuffer) -> *const u8
string_as_ptr(&StringBuffer) -> *const u8
string_len(&StringBuffer) -> i64
string_validate_utf8(ptr: *const u8, len: i64) -> bool
```
`string_buffer_push_str` takes three arguments. It still falls back to `strlen`
when the length is non-positive, which keeps binaries built by older compilers
working, but a 2-argument call is now a compile error.

### `std.io`
```morrow
io_print(msg: String) yields [IO]
i64_to_string_in(val: i64, arena: *mut u8) -> String
buf_reader_new(content: String) -> BufReader
buf_reader_read_line_in(&mut BufReader, arena) -> String
buf_writer_new_in(arena, initial_cap) -> BufWriter
buf_writer_write_str(&mut BufWriter, s: String)
buf_writer_write_i64(&mut BufWriter, val: i64)
buf_writer_to_string(&mut BufWriter) -> String
```

### `std.http`
```morrow
http_parse_request_in(raw: String, arena: *mut u8) -> HttpRequest
http_response_ok(body: String, content_type: String) -> HttpResponse
http_response_created(body: String) -> HttpResponse
http_response_bad_request(msg: String) -> HttpResponse
http_response_not_found() -> HttpResponse
http_format_response_in(&HttpResponse, arena) -> String
```

### `std.net`
```morrow
tcp_listen(port: Port) -> TcpListener
tcp_accept(&TcpListener) -> TcpStream
tcp_connect(host: String, port: Port) -> TcpStream
tcp_read(&TcpStream, max_len: i64) -> String
tcp_write(&TcpStream, data: String) -> i64
tcp_close(&TcpStream) -> ()
```
All six operations are effects (`effect Net`), so callers must declare
`yields [Net]` or wrap the work in a handler.

### `std.fs` / `std.process`
```morrow
fs_read_to_string(path: String) -> String yields [FS]
fs_read_to_string_in(path: String, arena: *mut u8) -> String yields [FS]   // path first
fs_write_string(path, content) -> bool yields [FS]
fs_exists(path) -> bool yields [FS]
fs_size(path) -> i64 yields [FS]
fs_delete(path) -> bool yields [FS]

process_exec(cmd: String, args: String) -> i64 yields [Process]
command_new(program) -> Command
command_arg(&mut Command, arg) 
command_status(&Command) -> i64 yields [Process]
```
Caveat (v2.0.1): `std::fs` (`fs_read*`, `fs_write_string`, `fs_exists`,
`fs_size`, `fs_delete`) and `process_exec` are backed by codegen runtime thunks and
work. `command_new` / `command_arg` / `command_status` are **not** emitted into the
object file, so programs using them fail at link with
`undefined value '@command_arg'`; use `process_exec(cmd, args)` instead.

### `std.sqlite` / `std.sql` / `std.sql_builder`
```morrow
sqlite_open(path: String) -> SqliteDb
sqlite_close(&mut SqliteDb)
sqlite_execute_prepared(&mut SqliteDb, sql: String, p1: String, p2: String) -> i64
sqlite_query_in(&mut SqliteDb, sql: String, arena: *mut u8) -> SqliteResultSet
sqlite_row_count(&SqliteResultSet) -> i64
```
`std.sql` and `std.sql_builder` add a typed record wrapper and a fluent
`QueryBuilder`; see `tests/sql_test.mw`.

### `std.crypto`
```morrow
hex_encode_in(bytes: *const u8, len: i64, arena) -> String
hex_decode_in(hex_str: String, arena) -> *mut u8
fnv1a_hash(String) -> i64   adler32_hash(String) -> i64   crc32_hash(String) -> i64
rng_new(seed: i64) -> CryptoRng      rng_next_u64(&mut CryptoRng) -> i64
rng_next_range(&mut CryptoRng, min, max) -> i64
```

### `std.time` (benchmarking)
```morrow
time_now_ms() -> i64                      // WinMM timer, 1 ms resolution
time_sleep_ms(ms: i64)
stopwatch_start() -> Stopwatch
stopwatch_elapsed_ms(&Stopwatch) -> i64
stopwatch_reset(&mut Stopwatch)
time_ops_per_sec(iterations: i64, elapsed_ms: i64) -> i64
```

### Printing
`println` is a codegen intrinsic supporting three shapes:
```morrow
println("a string literal");        // puts(literal)
println(some_integer);              // printf("%lld\n", value)
println("count = {}", some_value);  // one {} placeholder (string or integer)
```
Only a **single** `{}` placeholder is substituted, and only when the first argument
is a string literal; further arguments are ignored. Use one value per call for
anything more elaborate.


---

## 13. Idiomatic Patterns (compiled and executed)

### A request handler with a mock Net effect
No nurseries or fibers exist; you write the request path as an effectful function
and supply a handler (real sockets in production, a mock in tests).
```morrow
import std::net;
import std::http;

fn serve_one(conn_id: i64) -> i64 yields [Net] {
    let stream = tcp_connect("127.0.0.1", 8080 as Port);
    let mut written: i64 = 0;
    region req_r {
        let raw = tcp_read(&stream, 4096);
        let req = http_parse_request_in(raw, req_r);
        if !req.is_valid {
            let bad = http_response_bad_request("{\"error\":\"bad request\"}");
            let wire = http_format_response_in(&bad, req_r);
            written = tcp_write(&stream, wire);
        } else {
            let ok = http_response_ok("{\"status\":\"ok\"}", "application/json");
            let wire = http_format_response_in(&ok, req_r);
            written = tcp_write(&stream, wire);
        }
        tcp_close(&stream);
    }
    written
}

fn main() -> i64 {
    let handled = handle {
        serve_one(42)
    } with Net {
        fn listen(k, port: Port) { resume(k, 1); }
        fn accept(k, listener: i64) { resume(k, 2); }
        fn connect(k, host: String, port: Port) { resume(k, 3); }
        fn read(k, conn: i64, max_bytes: i64) { resume(k, "GET /health HTTP/1.1\r\nHost: x\r\n\r\n"); }
        fn write(k, conn: i64, data: String) { resume(k, 1); }
        fn close(k, conn: i64) { resume(k, 0); }
    };
    handled      // 1
}
```
Verified: compiles and returns 1.

### Region-scoped SQLite query
```morrow
import std::sqlite;

fn query_users(db_path: String) -> i64 yields [FS, Database] {
    let mut db = sqlite_open(db_path);
    let inserted = sqlite_execute_prepared(&mut db, "INSERT INTO users (name, role) VALUES (?, ?)", "Alice", "Admin");
    let mut count: i64 = 0;
    region q {
        let rows = sqlite_query_in(&mut db, "SELECT id, name FROM users", q);
        count = sqlite_row_count(&rows);
    }
    sqlite_close(&mut db);
    if inserted < 0 { return 0 - 1; }
    count
}
```
Notes: `sqlite_open` must be bound to `let mut db` because the other calls take
`&mut SqliteDb`; the result set is region-allocated and dies with the region.

---

## 14. Not Implemented / Known Limitations

Parsed as errors (do not appear in the language today):

| Construct | Status / symptom |
|---|---|
| `3.14` (float literal) | parse error (`Expected field name after '.'`) |
| `enum` / `match` | `Unexpected token at top level` |
| `for x in ...` | parse error |
| `nursery` / `n.spawn` | parse error |
| `if` as an expression (`let x = if c {1} else {2};`) | `Expected expression` |
| closures (`let f = \|x\| ...`) | parse error |
| `continue` (outside a loop) | silently ignored (no enclosing loop to jump to) |
| handler shorthand `op(args) => resume(...)` | parse error; use `fn op(k, args) { resume(k, v); }` |

Implemented but weaker than older drafts claimed:

- **Refinements**: literal casts and divisor-span checks only; no narrowing, no
  variable-range proofs.
- **Affine resources**: no auto-drop; consumption must happen on every path.
- **Escape analysis**: not enforced (escaping region references compile).
- **`println`**: one value per call, no `{}` interpolation.
- **Call signatures**: callee existence and argument **count** are enforced
  (v2.0.2); argument *types* are not compared, because `tc_expr` currently infers
  `i64` for most expressions — a kind comparison produced 152 false positives across
  the compiler/stdlib during development, so type checking needs real inference
  first.
- **`forge check`**: runs the full typechecker (including callee existence/arity)
  but does not link, so a missing C symbol is only caught by `forge build`.
- **`forge fmt --check`**: the formatter is idempotent, but the repository's own
  sources are not formatted in its canonical style, so running `--check` over the
  tree reports every file as unformatted. Format before enforcing it in CI.
- **Debug information / LTO**: no CodeView/PDB emission, no LTO or
  function-section GC (older roadmap entries marked these done — they are not).
- **`std::process`**: `process_exec` is lowered to a runtime thunk and works.
  `command_new` / `command_arg` now link and run (the module loader used to skip
  `std/process.mw`), but `command_status` crashes at runtime because the `Process`
  **effect has no working default handler** — `perform Process.spawn(...)` faults.
  Use `process_exec` until effect handlers for the built-in effects are wired up.
  (`std::fs` read/write/exists/size/delete are all backed by thunks and work.)

Toolchain requirements:

- `clang` must be on `PATH`; Windows linking needs the MinGW CRT objects in
  `lib/crt`. "Zero external toolchain dependencies" is therefore **not** accurate —
  the compiler is self-hosted, the *linker* is not.

---

## 15. How This Reference Was Verified

Every claim above was checked against the released compiler with a probe suite:

1. **Language surface** — ~35 single-feature programs (`region`, effects, generics,
   linear/affine, refinements, literals, control flow) built with
   `forge build`; results classified as COMPILES / PARSE_ERROR / TYPECHECK_ERROR /
   LINK_ERROR.
2. **Stdlib signatures** — each documented name compared against its `pub fn`
   declaration in `std/*.mw`, and wrong-arity/wrong-name variants compiled to show
   the checker's blind spots.
3. **Runtime behaviour** — `println`, `StringBuffer`, refinement rejection and
   effect plumbing were executed, not just compiled (exit codes and stdout checked).
4. **Tooling** — `forge resolve` on a two-level dependency graph (lockfile captured
   verbatim above), `--target` builds checked by magic bytes, and every shipped
   example built with the released binary.
5. **Release gates** — 22/22 native suites, 13 compile-fail suites
   (`tests/negative/*.mw`, asserted by `forge test`), benchmark suite, and the
   3-stage self-hosting fixed point.

