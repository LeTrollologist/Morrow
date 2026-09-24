# Morrow Language & Syntax Specification (v2.0)

> **Morrow** is a modern systems-level programming language designed to provide fearless concurrency and zero-cost abstractions, replacing explicit lifetime annotations with **Region-Based Memory Management**, eliminating the async/await function coloring divide through **Algebraic Effects**, eliminating resource leaks through **Linear & Affine Types**, and eliminating runtime out-of-bounds panics through **Compile-Time Refinement Types**.

---

## Table of Contents

1. [Lexical Grammar](#1-lexical-grammar)
2. [Type System & Refinements](#2-type-system--refinements)
3. [Linear & Affine Resource Types](#3-linear--affine-resource-types)
4. [Memory Model: Scoped Regions](#4-memory-model-scoped-regions)
5. [Colorless Algebraic Effects](#5-colorless-algebraic-effects)
6. [Structured Concurrency & Nurseries](#6-structured-concurrency--nurseries)
7. [Declarations & Items](#7-declarations--items)
8. [Statements & Control Flow](#8-statements--control-flow)
9. [Foreign Function Interface (FFI) & Unsafe](#9-foreign-function-interface-ffi--unsafe)
10. [Package Management & Locking](#10-package-management--locking)
11. [Multi-Target Compilation](#11-multi-target-compilation)
12. [Standard Library Reference](#12-standard-library-reference)
13. [Idiomatic Code Patterns](#13-idiomatic-code-patterns)

---

## 1. Lexical Grammar

### Comments
```morrow
// Single-line comment to end of line

/* Multi-line
   block comment */
```

### Identifiers
Identifiers must start with an ASCII letter or underscore, followed by any alphanumeric character or underscore:
```morrow
var count = 0;
let user_id = 42;
let _scratch = "temp";
```

### Literals
- **Integers**: Decimal (`42`, `100_000`), Hexadecimal (`0xFF`, `0x8004667e`).
- **Floats**: Standard decimal floats (`3.14159`, `0.5`).
- **Booleans**: `true`, `false`.
- **Strings**: Double-quoted UTF-8 strings (`"Hello, world!\n"`).
- **Null pointer**: `null`.

### Keywords
```text
fn        var       let       if        else      while     for       in
return    match     struct    enum      type      import    const
region    nursery   effect    yields    handle    with      resume    perform
linear    affine    consume   drop      extern    unsafe    true      false
null      as
```

---

## 2. Type System & Refinements

Morrow is statically typed with bidirectional type inference and a compile-time arithmetic interval solver.

### Primitive Types
| Type | Width | Description |
| :--- | :---: | :--- |
| `i8`, `i16`, `i32`, `i64` | 8–64 bit | Signed two's-complement integers |
| `u8`, `u16`, `u32`, `u64` | 8–64 bit | Unsigned integers |
| `f32`, `f64` | 32, 64 bit | IEEE-754 floating-point numbers |
| `bool` | 1 bit | Boolean value (`true` or `false`) |
| `String` | 24 bytes | Heap-allocated or region-allocated UTF-8 string |
| `void` | 0 bytes | Unit / empty return type |

### Refinement Types
Refinement types decorate primitive integers with mathematically verified invariant intervals `[min..max]`. The compiler mathematically proves bounds at compile time, eliminating runtime bounds checks in generated machine code:

```morrow
// Range interval refinements:
type Percentage = u8[0..100];
type Port = u16[1..65535];
type HttpStatusCode = u16[100..599];

// Compile-time verified instantiation:
var valid_port: Port = 8080 as Port; // OK: 8080 in [1..65535]
var bad_port: Port = 70000 as Port;  // Compile-time error: 70000 outside [1..65535]
```

#### Relational Refinements
Refinement intervals can reference other variables in scope to enforce relational contracts:
```morrow
// Enforces that 'end' must be greater than or equal to 'start'
fn subslice_len(start: usize, end: usize(>= start)): usize {
    end - start // Guaranteed non-negative, zero bounds check emitted
}
```

#### Non-Linear Arithmetic & Provable Zero-Divisor Safety
Morrow's interval solver evaluates mixed-sign 4-point extremal multiplication and provable non-zero divisor safety:
```morrow
type Divisor = i64[1..100]; // Strictly positive, does not span 0
fn safe_divide(x: i64, d: Divisor): i64 {
    x / d // Guaranteed non-zero divisor, division-by-zero panic statically impossible
}
```
If a divisor interval spans zero (e.g. `[-5..5]`), the compiler statically rejects the operation at compile time.

#### Path-Sensitive Interval Narrowing
Conditional expressions narrow variable interval bounds within conditional branches and restore original bounds upon exit:
```morrow
fn process_score(score: i64) {
    if score >= 0 && score <= 100 {
        // Here, 'score' is automatically narrowed to refinement interval [0..100]
        let valid_percentage: Percentage = score as Percentage;
    }
}
```

### References and Pointers
```morrow
&T          // Immutable borrowed reference
&mut T      // Mutable borrowed reference
*mut T      // Raw mutable pointer (C-compatible FFI)
*const T    // Raw constant pointer (C-compatible FFI)
```

### Parametric Generics with Call-Site Monomorphization

Morrow implements parametric generics via ahead-of-time call-site monomorphization rather than runtime polymorphism or type erasure. Generic templates for structs and functions are instantiated into concrete specializations prior to typechecking and code generation.

Conceptual lowering pipeline:

```text
foo::<i64>(...)
       │
       ▼
foo__i64
       │
       ▼
concrete TIR
       │
       ▼
LLVM
```

Generic call arguments and struct instantiations require explicit turbofish syntax (`::<Type>`):

```morrow
struct Pair<T> {
    first: T,
    second: T,
}

fn identity<T>(item: T): T {
    item
}

// Specializations instantiated ahead of codegen:
// identity::<i64>(42)        -> instantiates and calls identity__i64(42)
// Pair::<i64> { first: 10, second: 20 } -> instantiates and constructs Pair__i64
```

---

## 3. Linear & Affine Resource Types

Morrow provides compile-time ownership semantics for OS handles, hardware peripherals, and critical system resources through **linear** and **affine** structs.

### Linear Structs (`linear struct`) — Exactly-Once Consumption
A linear struct represents a resource that must be consumed **exactly once**. If a linear resource is dropped or falls out of scope unconsumed, the compiler raises a compile-time leak error:

```morrow
linear struct FileHandle {
    fd: i64,
}

fn close_file(f: FileHandle) {
    consume(f); // Zero-cost intrinsic: marks resource consumed
}

fn leak_violation(f: FileHandle) {
    // COMPILE ERROR: Linear Resource Violation: resource dropped without being consumed
}
```

#### Branch Convergence
Linear resources must reach identical consumption states across all conditional control-flow branches:
```morrow
fn branch_check(cond: bool, f: FileHandle) {
    if cond {
        close_file(f);
    } else {
        close_file(f); // Must be consumed along both paths
    }
}
```

### Affine Structs (`affine struct`) — At-Most-Once Consumption
An affine struct represents a resource that can be consumed **at most once**. If unconsumed, it safely auto-drops at scope exit without error:

```morrow
affine struct TempBuffer {
    ptr: *mut u8,
}

fn use_temp(b: TempBuffer, early_exit: bool) {
    if early_exit {
        return; // OK: Affine resource safely auto-drops at scope exit
    }
    consume(b);
}
```

### Resource Intrinsics
- `consume(res)`: Explicitly marks a linear or affine resource as consumed. Any subsequent access triggers a compile-time `use-after-consume` violation.
- `drop(res)`: Explicitly discards an affine resource.

---

## 4. Memory Model: Scoped Regions

Morrow replaces explicit lifetime annotations with **Lexical Memory Regions**.

### The `region` Expression
```morrow
region r {
    var buf = string_buffer_new_in(r);
    string_buffer_push_str(buf, "Allocated in bump arena 'r'");
    println("{}", string_buffer_as_ptr(buf));
} // O(1) Bulk Teardown: Entire arena memory reclaimed instantly here
```

### Region-Aware Allocation (`_in` convention)
Standard library collections support dual allocation strategies:
1. System Heap: `vec_new()`, `hashmap_new(cap)`, `string_buffer_new()`.
2. Scoped Region: `vec_new_in(r)`, `hashmap_new_in(r, cap)`, `string_buffer_new_in(r)`.

```morrow
region r {
    // 10,000 table rows allocated inside region 'r'
    var users = sqlite_query_in(db, "SELECT id, name FROM users", r);
    println("Fetched {} users without heap fragmentation", sqlite_row_count(users));
} // All rows and string buffers dropped in 0.00ms
```

### Linear Escape Analysis
Morrow's compiler enforces strict lexical safety. References to data within a region cannot outlive that region:
```morrow
fn illegal_escape(): &String {
    region r {
        var s = string_new_in(r, "hello");
        &s // COMPILE ERROR: Reference to region 'r' escapes local scope
    }
}
```

---

## 5. Colorless Algebraic Effects

Morrow eliminates the `async`/`await` and `Result`/`Option` function coloring divide by using **Delimited Algebraic Effects**.

### Effect Declarations
```morrow
effect Database {
    fn query(sql: String) -> String;
    fn execute(sql: String) -> i64;
}

effect Logger {
    fn log(msg: String) -> i64;
}
```

### Effect Capability Annotations (`yields [...]`) & Operations (`perform`)
Functions declare which effects they may yield during execution, and invoke operations via `perform`:
```morrow
fn find_user(id: i64) -> String yields [Database, Logger] {
    perform Logger.log("Searching database...\0");
    perform Database.query("SELECT name FROM users WHERE id = 1\0")
}
```

### Delimited Effect Handlers
Callers handle effects explicitly using `handle { ... } with Effect { fn op(k, ...) { ... } }`, choosing between real production drivers, in-memory mocks, or async runtimes:
```morrow
fn main() -> i64 {
    let name = handle {
        handle {
            find_user(101)
        } with Logger {
            fn log(k, msg: String) {
                // Log and resume caller
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
    0
}
```

---

## 6. Structured Concurrency & Nurseries

Morrow provides native structured concurrency through **Nurseries** and **Fibers**, multiplexed over an M:N worker task pool driven by kernel-level completion ports (Win32 IOCP).

### The `nursery` Scope
A nursery defines a deterministic lexical lifecycle for concurrent tasks. When execution exits the nursery block, it waits for all spawned fibers to complete before continuing:
```morrow
fn task_worker(task_id: i64, dummy: i64) yields [IO] {
    println("Executing fiber task #{}", task_id);
}

fn main() yields [Async, IO] {
    nursery n {
        n.spawn(task_worker, 1, 0);
        n.spawn(task_worker, 2, 0);
        n.spawn(task_worker, 3, 0);
    } // Deterministic join: Blocks until all 3 tasks terminate

    println("All concurrent fibers completed successfully");
}
```

### Channels & Message Passing
```morrow
var ch = channel_open(10); // Bounded channel with capacity 10

channel_send_msg(ch, 42);
var val = channel_recv_msg(ch); // val == 42
```

---

## 7. Declarations & Items

### Functions
```morrow
fn add(a: i64, b: i64): i64 {
    a + b
}

fn procedure(flag: bool): void {
    if flag {
        println("True");
    }
}
```

### Structs
```morrow
struct User {
    id: i64,
    name: String,
    is_admin: bool,
}

// Instantiation:
var u = User {
    id: 1,
    name: "Morrow Admin",
    is_admin: true,
};

// Field access:
var uid = u.id;
```

### Enums & Pattern Matching
```morrow
enum HttpResponse {
    Ok(String),
    BadRequest(String),
    NotFound,
}

fn render(resp: HttpResponse): void {
    match resp {
        HttpResponse.Ok(body) => println("200: {}", body),
        HttpResponse.BadRequest(err) => println("400: {}", err),
        HttpResponse.NotFound => println("404: Not Found"),
    }
}
```

### Type Aliases
```morrow
type UserId = i64;
type Port = u16[1..65535];
type StringList = Vec;
```

### Imports
```morrow
import std.net;
import std.http;
import std.sqlite;
import std.collections;
import std.fs;
import std.process;
```

---

## 8. Statements & Control Flow

### Variables & Mutability
```morrow
let immutable_x = 10;
// immutable_x = 20; // Compile-time error

var counter = 0;
counter = counter + 1; // OK
```

### Conditional Branches (`if / else`)
```morrow
if x > 100 {
    println("Large");
} else if x > 50 {
    println("Medium");
} else {
    println("Small");
}
```

### Loops
```morrow
// While loop:
var i = 0;
while i < 10 {
    i = i + 1;
}

// Infinite loop:
while true {
    // break or exit
}
```

---

## 9. Foreign Function Interface (FFI) & Unsafe

Morrow seamlessly binds to native C libraries without runtime wrappers using `extern "C"`.

### C-ABI Declarations
```morrow
extern "C" {
    fn malloc(size: i64): *mut u8;
    fn free(ptr: *mut u8): void;
    fn exit(code: i32): void;
    fn sqlite3_open(filename: *const u8, ppDb: *mut *mut u8): i32;
}
```

### Unsafe Blocks
Operations involving raw pointer dereferencing or C-ABI function calls require an `unsafe` block:
```morrow
unsafe {
    var ptr = malloc(64);
    // Perform low-level C memory operations
    free(ptr);
}
```

---

## 10. Package Management & Locking

Morrow's package system manages modular project manifests and guarantees deterministic dependency resolution.

### Package Manifest (`Forge.toml`)
Every package defines a root `Forge.toml`:
```toml
[package]
name = "my_service"
version = "0.1.0"
edition = "2026"

[dependencies]
math_core = { path = "../math_core", version = "^1.2.0" }
net_utils = { path = "../net_utils", version = "0.4.*" }
```

### Semantic Versioning Rules (SemVer Subset)
Morrow supports standard numeric `X.Y.Z` SemVer constraints:
- Exact: `=1.2.3` or `1.2.3`
- Caret compatibility:
  - `^1.2.3` $\implies [1.2.3, 2.0.0)$
  - `^0.2.3` $\implies [0.2.3, 0.3.0)$
  - `^0.0.3` $\implies [0.0.3, 0.0.4)$
- Wildcards: `*`, `1.*`, `1.2.*`

### Lockfile Format & Graph Closure (`Forge.lock`)
`forge resolve` outputs a canonical `Forge.lock`. Packages and outbound dependencies are sorted alphabetically to ensure byte-identical determinism:
```toml
# This file is automatically generated by Forge.
# Do not edit manually.

[[package]]
name = "math_core"
version = "1.2.4"
source = "local"
path = "../math_core"
dependencies = [
    "algebra_lib 0.8.1",
]

[[package]]
name = "algebra_lib"
version = "0.8.1"
source = "local"
path = "../algebra_lib"
dependencies = []
```
**Graph Closure Invariant:** Every dependency listed in `dependencies = [...]` must exist as a top-level `[[package]]` in `Forge.lock` with a matching version.

---

## 11. Multi-Target Compilation

The self-hosted compiler supports native code generation and cross-compilation across multiple target architectures via `--target`:

| Target Triple | Output Binary | ABI & Delimited Continuation Details |
| :--- | :--- | :--- |
| `x86_64-pc-windows-gnu` (default) | Windows PE `.exe` | Microsoft x64 ABI (`%rcx`, `%rdx`, `%r8`, `%r9`), CodeView debug info |
| `x86_64-unknown-linux-gnu` | Linux ELF executable | System V AMD64 ABI (`%rdi`, `%rsi`, `%rdx`), POSIX shims (`/proc/self/cmdline`) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 ELF | AAPCS64 ABI (`x19–x28`, `x29`, `x30` link register, `d8–d15`) |
| `wasm32-unknown-unknown` | WebAssembly `.wasm` | Self-contained 32-bit WASM IR, Node.js & browser host interoperability |

```powershell
mwc file.mw --target wasm32-unknown-unknown -o file.wasm
```

---

## 12. Standard Library Reference

### 1. `std.collections`
- **`HashMap`**: Cache-conscious Robin Hood hash map using Structure-of-Arrays (SoA) layout and FNV-1a hashing. Supports dual allocators (`hashmap_new` / `hashmap_new_in`).
- **`Vec`**: Contiguous dynamic array (`vec_new`, `vec_new_in`, `vec_push`, `vec_pop`, `vec_get`, `vec_set`, `vec_len`).
- **`StringBuffer`**: Region-backed mutable string builder (`string_buffer_new_in`, `string_buffer_push_str`, `string_buffer_as_ptr`).

### 2. `std.http`
- **Request Parsing**: `http_parse_request_in(raw, arena): HttpRequest`.
- **Response Formatters**:
  - `http_response_ok(body, content_type): HttpResponse`
  - `http_response_created(body, content_type): HttpResponse`
  - `http_response_bad_request(body): HttpResponse`
  - `http_response_not_found(): HttpResponse`
- **Wire Serialization**: `http_format_response_in(resp, arena): *mut u8`.

### 3. `std.sqlite`
- **Connection**: `sqlite_open(path): SqliteDb`, `sqlite_close(db)`.
- **Parameterized Execution**: `sqlite_execute_prepared(db, sql, param1, param2): bool` (Zero SQL injection risk).
- **Region Queries**: `sqlite_query_in(db, sql, arena): SqliteRow` (Zero per-row heap allocations).

### 4. `std.net`
- **Listener**: `tcp_listen(port: Port): TcpListener`.
- **Accept**: `tcp_accept(listener: &TcpListener): TcpStream`.
- **I/O**: `tcp_read(stream: &TcpStream, max_len: i64): *mut u8`, `tcp_write(stream: &TcpStream, data: *mut u8): i64`.
- **Close**: `tcp_close(stream: &TcpStream): void`.

### 5. `std.fs` & `std.process`
- **Filesystem**: `fs_read_to_string_in(arena, path)`, `fs_write_string(path, content)`, `fs_exists(path)`, `fs_delete(path)`.
- **Process Orchestration**: `command_new(prog)`, `command_arg(cmd, arg)`, `command_status(cmd): i32`.

---

## 13. Idiomatic Code Patterns

### Pattern A: Fortress v2 High-Concurrency HTTP Server
Combines structured concurrency nurseries, fixed M:N worker task pools, Win32 IOCP, and scoped request memory regions:

```morrow
import std.net;
import std.http;
import std.collections;

fn client_worker(conn_id: i64, dummy: i64) yields [Net] {
    var stream = TcpStream {
        socket_id: conn_id,
    };
    region req_r {
        var raw = tcp_read(&stream, 4096);
        var req = http_parse_request_in(raw, req_r);

        if !req.is_valid {
            var bad = http_response_bad_request("{\"error\":\"Invalid HTTP Request\"}");
            var wire = http_format_response_in(&bad, req_r);
            tcp_write(&stream, wire);
        } else if req.method == "GET" && req.path == "/health" {
            var resp = http_response_ok("{\"status\":\"ok\"}", "application/json");
            var wire = http_format_response_in(&resp, req_r);
            tcp_write(&stream, wire);
        } else {
            var not_found = http_response_not_found();
            var wire = http_format_response_in(&not_found, req_r);
            tcp_write(&stream, wire);
        }
        tcp_close(&stream);
    } // O(1) bulk memory reclamation on client finish
}

fn main() yields [Net, IO, Async] {
    println("=== Starting Fortress v2 Server ===");
    var port: Port = 8080 as Port;
    var listener = tcp_listen(port);

    nursery server_n {
        while true {
            var stream = tcp_accept(&listener);
            if stream.socket_id > 0 {
                server_n.spawn(client_worker, stream.socket_id, 0);
            }
        }
    }
}
```

### Pattern B: SQL-Injection-Proof Region Query Pipeline
```morrow
import std.sqlite;
import std.collections;

fn query_users_safe(db_path: String) yields [Database, IO] {
    var db = sqlite_open(db_path);

    // 1. Parameterized execution (immune to SQL injection)
    sqlite_execute_prepared(db, "INSERT INTO users (name, role) VALUES (?, ?)", "Alice", "Admin");

    // 2. Region-scoped bulk query
    region query_r {
        var rows = sqlite_query_in(db, "SELECT id, name, role FROM users", query_r);
        var count = sqlite_row_count(rows);
        println("Fetched {} users in region", count);
    } // All rows dropped instantly without heap fragmentation

    sqlite_close(db);
}
```
