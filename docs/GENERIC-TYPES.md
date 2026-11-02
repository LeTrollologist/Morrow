# Generic instantiation: does a reusable representation exist?

**Investigation for v2.4.15. No code was changed.** The question: does monomorphization
already preserve concrete generic arguments in a form the typechecker can reuse?

Short answer: **no — and the reason is not the monomorphizer.** Generic arguments are
destroyed before monomorphization ever sees them.

## What exists today (evidence)

| Fact | Where |
|---|---|
| Build order is `desugar_handlers` → **`monomorphize_program`** → `typecheck_program` | `compiler/forge.mw:875-878` |
| Monomorphization mangles names as `base__concrete` into a fresh interned symbol and rewrites call sites to it; dedup by mangled name | `compiler/forge.mw:278-287` (`mangle_sym`), invariants 1-3 at `701-711` |
| Specializations are **clones appended** to `prog.functions` with `type_param_sym = 0` | `compiler/forge.mw:684-699`, `compiler/typeck.mw:1924` |
| Typechecking **skips** generic templates, because the clones are what get checked | `compiler/typeck.mw:1905`, `1916-1930` |
| **`parse_type` consumes `<...>` and returns only the base symbol** | `compiler/parser.mw:117-131` |
| The turbofish of a *struct literal* is kept in `expr.int_val` until monomorphization clears it | `compiler/parser.mw:347`, `compiler/forge.mw:708-711` |
| `check`, `argkinds` and LSP call `typecheck_program` **without** monomorphizing | `compiler/forge.mw:1018`, `:1000`, `compiler/lsp.mw:325` |

The critical line is the parser one:

```morrow
// parse_type, on seeing `Vec<i64>`:
if check(p, tok_lt()) {
    advance(p);                              // consume <
    let mut depth: i64 = 1;
    while depth > 0 && !check(p, tok_eof()) { ... advance(p); }
}                                            // ...and the tokens are gone
```

A type annotation `Vec<i64>`, a parameter `v: &mut Vec<T>` and a return `-> Vec<T>` all
collapse to the same symbol `Vec` **at parse time**. Nothing downstream — monomorphizer,
TIR, codegen — ever receives a `Vec<i64>` annotation, so no later phase could have recorded
one.

## The three criteria

| Criterion | Answer | Why |
|---|---|---|
| 1. Do `Vec<String>` and `Vec<i64>` have distinct identities? | **No**, for types | Parser erases the arguments; `TcType` has `kind`/`sym`/`left`/`right` and no type-argument field. Distinct identities exist only for *functions*, as mangled name symbols (`identity__i64`). |
| 2. Do those identities survive parameters, calls and returns? | **No** | A call keeps its turbofish in `expr.int_val` until monomorphization rewrites it, but parameters and return types never had it. |
| 3. Can the typechecker reach them without a later-phase dependency? | **Not in the current form** | Monomorphization runs *earlier* than typechecking on the build path (good), but it is **not run** on `check`/`argkinds`/`lsp`, and its output is a *mutated AST*, not a type table. Depending on it would silently change what `forge check` means. |

So the monomorphizer's scheme is a good *idea* — identity via a fresh interned symbol — but
it cannot be reused as-is: its symbols describe *specializations of functions*, and it never
sees type annotations at all.

## Three candidate designs

| | Design | Identity | Survives annotations? | Available on all paths? | Cost |
|---|---|---|---|---|---|
| A | Reuse monomorphizer output (mangled fn syms + clones) | functions only | no | no (`check`/`lsp` skip it) | moves typechecking behind a pre-pass, changes `check` semantics |
| B | **Intern the concrete type name in `parse_type`** (`Vec<i64>`, `&mut Vec<String>` → one symbol) | yes, per concrete type | yes, all annotations come from `parse_type` | yes, produced before everything | parser change + `tc_resolve_type` fallback to the template by stripping arguments |
| C | Type-argument list on `TcType` (e.g. `args: Vec<i64>` of syms) | yes | yes, if propagated everywhere | yes | touches every `TcType` allocation and every comparison; largest blast radius |

**Recommendation: B.** It reuses the mechanism the compiler already trusts — the interner
guarantees that identical strings map to the same symbol, which is exactly the invariant the
monomorphizer relies on for `base__concrete`. Type names would carry `<...>` while function
symbols carry `__`, so the two schemes stay visually distinct. `TcType.sym` already holds a
name symbol, so **no `TcType` change is required**: `tc_resolve_type` gains one fallback
(`Vec<i64>` → look up `Vec` for the field list), and `struct_defs`/`types` stay keyed as they
are today.

## Risks to settle before implementing

1. **Refinement aliases flow through the same function.** `parse_type` also handles
   `u8[0..100]`; the change must keep intervals working and must not turn `[0..100]` into
   part of a type name.
2. **`tc_resolve_type` has two maps** (`types`, `struct_defs`) keyed by plain name. Every
   instantiated name will miss both until the fallback exists, so the two changes are one
   unit, not two.
3. **Duplicate identity**: `Vec<i64>` interned in the parser and `Vec__i64` produced by
   monomorphization are different symbols for the same concept. That is acceptable (they
   live in different phases and never meet), but it must be documented or someone will
   conflate them later.
4. **`&mut Vec<T>`** — the parser currently strips both `&mut` and `<...>`; preserving
   `&mut Vec<String>` as a name must not change how `ty_ref`/`ty_ptr` kinds are produced,
   because the region-escape rules depend on those kinds.

## What the corpus will decide

Cases 16/17/19 are already the discriminating triple: the same call `vec_get(&v, 0)` must
report `String` for a `Vec<String>` and `i64` for a `Vec<i64>`, and must keep reporting the
deliberately wrong pairing. A design that cannot distinguish those three at the typechecker
has not met the requirement, regardless of how the aggregate count moves.
