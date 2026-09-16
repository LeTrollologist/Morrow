# Tungsten

**Tungsten** is a systems-level programming language designed to provide fearless concurrency and zero-cost abstractions, replacing explicit lifetimes with **Region-Based Memory Management** and replacing the async/await divide with an **Algebraic Effect System**.

For development status and upcoming milestones, see [ROADMAP.md](ROADMAP.md).

---

## Core Differentiators

1. **Region-Based Memory Management (No explicit `<'a>` lifetimes)**  
   Allocations are grouped into compile-time inferred regions. When a region exits scope, memory is reclaimed en masse without lifetime annotations polluting struct and function definitions. Linear and affine typing guarantees reference validity.

2. **Colorless Functions via Algebraic Effects**  
   No `async` or `await` infection. Functions simply declare effect capabilities (`yields [Db, IOError]`). Callers choose how effects are handled: synchronous execution, event-loop suspension, or pure unit-test mocking.

3. **Refinement Types (Compile-time verified bounds)**  
   Declare bounded types like `type Health = u8(0..=100);`. The compiler mathematically proves arithmetic constraints at compile time, eliminating runtime out-of-bounds panics and bounds-checking overhead.

4. **High-Concurrency Async Engine (Fortress v2)**  
   M:N fiber task pool driven by kernel-level Win32 I/O Completion Ports (IOCP). Sustains 5,000+ simultaneous connections with < 1.2 KB RAM overhead per connection and zero thread quantum stalls.

5. **Contexts (Compile-Time Implicit Injection)**  
   Implicitly threaded capabilities flow down call graphs without manual parameter plumbing, verified safely at compile time.

---

## Workspace Architecture

```
Tungsten/
├── Cargo.toml
├── compiler/              # Self-Hosting Compiler Frontend (Stage 1 in Tungsten)
│   ├── ast.tg, lexer.tg, parser.tg, codegen.tg
│   ├── interner.tg, diagnostics.tg, main.tg
├── std/                   # Tungsten Standard Library
│   ├── prelude.tg, refinements.tg, effects.tg
│   ├── collections.tg     # Robin Hood HashMap, Vec, StringBuffer
│   ├── sqlite.tg          # Injection-proof parameterized driver
│   ├── http.tg            # Zero-copy HTTP/1.1 request/response engine
│   ├── fs.tg, process.tg, io.tg, net.tg
├── crates/
│   ├── tungsten-syntax/   # Lexer, AST, parser, and code formatter (fmt)
│   ├── tungsten-typeck/   # Type checker, Refinement Solver, Region Escape Analysis
│   ├── tungsten-tir/      # Basic-Block SSA IR, constant folding, bounds elimination
│   ├── tungsten-codegen/  # Native LLVM 18 IR emitter, M:N worker pool, JIT engine
│   ├── tungsten-fiber/    # Win32 IOCP completion port engine, fiber scheduler
│   ├── tungsten-vm/       # Delimited continuation effect runtime interpreter
│   ├── tungsten-fuzz/     # Differential fuzzing and allocator stress harness
│   ├── tungsten-lsp/      # Language Server Protocol (LSP) for VS Code / Neovim
│   └── forge/             # Developer CLI (`check`, `build`, `run`, `fmt`, `lsp`, `bindgen`)
├── docs/
│   └── SYNTAX.md          # Comprehensive Language & Syntax Specification
├── examples/
│   ├── web_service_v2.tg  # Fortress v2 Async IOCP Web Server (C100K engine)
│   ├── web_service.tg     # Fortress v1 REST API with SQLite & Region Sandboxing
│   ├── bootstrap_sample.tg# Milestone 3 Self-Hosting Test Sample
│   ├── sqlite_benchmark.tg# 10,000-row zero-allocation query benchmark
│   └── player.tg          # Flagship game demo with effects, refinement, and regions
```

For complete language grammar, types, and standard library reference, see [docs/SYNTAX.md](docs/SYNTAX.md).

---

## Developer Experience & Tooling

### 1. Build the Toolchain
```bash
cargo build --release
```

### 2. Format Source Code (`forge fmt`)
```bash
# Format in place
cargo run -p forge -- fmt examples/player.tg

# Validate formatting in CI
cargo run -p forge -- fmt --check examples/player.tg
```

### 3. Language Server (`forge lsp`)
Launch the Tungsten Language Server (supports VS Code, Neovim, Helix):
```bash
cargo run -p forge -- lsp
```
* **Real-time Diagnostics**: Red squigglies on refinement bounds violations and unhandled algebraic effects.
* **Hover Tooltips**: Shows inferred refinement intervals `[min..=max]` and algebraic effect rows `yields [...]`.
* **Format-on-Save**: Integrated formatting provider.

### 4. Cross-Compile for Linux (ELF)
Cross-compile any Tungsten program or microservice to a 64-bit Linux ELF binary from Windows:
```bash
cargo run -p forge -- build --target x86_64-unknown-linux-gnu examples/web_service_v2.tg
```

### 5. Production Docker Deployment
Deploy the Fortress v2 high-concurrency microservice in a secure, unprivileged (< 80 MB) Docker container:
```bash
docker compose up -d --build
```
Verify the zero-copy health probe:
```bash
curl http://127.0.0.1:8096/health
# {"status":"ok","server":"tungsten-fortress/2.0-iocp"}
```

---

## Syntax Showcase

```rust
// 1. Refinement Types
type Health = u8(0..=100);

struct Player {
    name: String,
    hp: Health,
}

// 2. Algebraic Effects instead of 'async' or 'Result'
fn fetch_player(id: u64) -> Player yields [Db, IOError] {
    let record = Db::query("SELECT * FROM players WHERE id = ?", id)?;
    
    Player {
        name: record.name,
        hp: record.hp as Health,
    }
}

// 3. Implicit Regions and Borrowing
fn heal_player(player: &mut Player, amount: u8) {
    player.hp = player.hp.saturating_add(amount);
}

// Entry point with Effect Handlers
fn main() {
    handle {
        let mut player = fetch_player(42)!;
        heal_player(&mut player, 20);
        println!("Player {} has {} HP", player.name, player.hp);
    } with Db {
        query(sql, args) => PostgresPool::execute(sql, args).await
    } with IOError {
        err => println!("Failed to fetch player: {}", err),
    }
}
```
