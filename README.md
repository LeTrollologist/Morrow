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

4. **Contexts (Compile-Time Implicit Injection)**  
   Implicitly threaded capabilities (like memory allocators and logger handles) flow down call graphs without manual parameter plumbing, verified safely at compile time.

---

## Workspace Architecture

```
Tungsten/
├── Cargo.toml
├── std/                   # Tungsten Standard Library (refinements & effects)
│   ├── prelude.tg
│   ├── refinements.tg
│   └── effects.tg
├── crates/
│   ├── tungsten-syntax/   # Lexer, AST, parser, and code formatter (fmt)
│   ├── tungsten-typeck/   # Type checker with Refinement Interval Solver & Effect Row tracker
│   ├── tungsten-vm/       # Runtime interpreter with delimited algebraic effect handlers
│   ├── tungsten-lsp/      # Language Server Protocol (LSP) for VS Code / Neovim
│   └── forge/             # Developer CLI (`check`, `run`, `fmt`, `lsp`, `new`)
├── examples/
│   ├── player.tg          # Flagship player game demo with effects, refinement, and regions
│   ├── std_demo.tg        # Standard library demonstration with Random & Time effects
│   └── refinement_error.tg# Diagnostic demo showing compile-time rejection of bad bounds & effects
```

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

### 4. Run Standard Library Showcase
```bash
cargo run -p forge -- run examples/std_demo.tg
```

Output:
```text
Player PlayerOne has 100 HP
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
