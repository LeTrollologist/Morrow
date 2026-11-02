# ARGKINDS corpus — validating the instrument before trusting its counts

`forge argkinds <file>` prints one line per call argument whose declared parameter kind
differs from the inferred argument kind:

```
ARGKIND want=<kind> got=<kind> :: <callee>
```

It never rejects anything. Because a report can reach zero for the wrong reason (an
inference system that is simply permissive), these cases pin what the instrument must say.
Readings below are from the **v2.4.12** binary; each case records an expected **count**.

## Two counts per case, because importing `std` adds its own findings

The report has no source positions (diagnostics are line 1 today), so it cannot say which
file a finding came from. Loading `std::collections` typechecks that module too, and its
own calls show up in the output. Each case therefore records:

- **own** — findings whose callee is declared in the case file. This is the number the case
  is actually about.
- **baseline** — findings contributed by imported modules at this version, named exactly,
  so a change in std becomes visible instead of being absorbed.

## Reference and generics

| Case | Area | Must | Own | Baseline | Reading |
|---|---|---|---|---|---|
| `case1_ref_to_struct` | `&p` → by-value `Probe` | silent (implicit deref) | 0 | 0 | ok |
| `case2_string_to_int` | `"hello"` → `i64` parameter | 1 finding | 1 | 0 | `want=int got=String :: want_int` |
| `case3_generic_string` | `id::<String>("hi")` | silent (expectation substituted) | 0 | 0 | ok |
| `case3b_generic_turbofish_mismatch` | `id::<i64>("hi")` | 1 finding, `want=int` | 1 | 0 | `want=int got=String :: id` — proves case 3 is not achieved by skipping generics |
| `case4_nested_ref` | `&&Probe` → by-value `Probe` | 1 finding (one level only) | 1 | 0 | `want=struct got=ref :: take` |
| `case5_ref_to_scalar` | `&i64` → `i64` parameter | silent (not a struct special case) | 0 | 0 | ok |

## Where information is lost, tested area by area

| Case | Area | Must | Own | Baseline | Reading |
|---|---|---|---|---|---|
| `case6_field_ok` | struct field, `String` → `String` | silent | 0 | 0 | field lookup **preserves** `String` |
| `case7_field_bad` | struct field, `i64` → `String` | 1 finding | 1 | 0 | `want=String got=int` |
| `case8_return_ok` | call result `String` → `String` | silent | 0 | 0 | the return type **is** resolved (`ret_ty_sym`) |
| `case9_return_bad` | call result `i64` → `String` | 1 finding | 1 | 0 | `want=String got=int` |
| `case10_accessor_ok` | `vec_get` → `i64` parameter | silent for the case | 0 | **1**: `want=String got=u8 :: hashmap_insert` | own call is silent; the std line is named, not hidden |
| `case11_accessor_bad` | `vec_len` → `String` parameter | 1 finding | 1 | **1**: same std line | `want=String got=int :: want_string` |
| `case12_variable_ok` | `let s: String` → `String` | silent | 0 | 0 | the declared kind survives the variable |
| `case13_variable_bad` | `let n: i64` → `String` | 1 finding | 1 | 0 | `want=String got=int` |

## What that says about the remaining 229

Field lookups, function returns and typed variables already carry their kind. The aggregate
count is dominated by **collection accessors and generic plumbing** — `vec_get` returning
`T`, `hashmap_insert` taking a `String` key — so it is one loss of information, not four.
The next fix therefore belongs in how `T`-typed callees report their results, *not* in a
rule that re-derives kinds at every argument.

## The rule this pins

A by-value parameter accepts `&x` / `&mut x`, so the comparison unwraps **one** level and
**only** when the parameter is not itself a reference; `&&T` against `T` stays reported.
When a parameter *is* the callee's generic parameter, the expectation comes from the
turbofish instantiation, and with no turbofish the argument is left **unreported** rather
than compared against the placeholder kind. The rule lives in the argument-kind report
only: `ty_ref` still carries its referent (`left`), and no other checker rule sees the
unwrapping.

## Model limits, stated rather than assumed

- **Mutability is not represented**: `&T` and `&mut T` are both `ty_ref`, so nothing here
  can distinguish them and nothing pretends to.
- **The comparison is kind-level**: `struct` vs `struct` never clashes, so `Vec` passed
  where `HashMap` is expected is not reported until substitution makes an `sym` comparison
  meaningful.
- **Finding attribution is by callee name**, because expressions carry no source position.
## Generic results — isolation cases (v2.4.13)

These six cases isolate *where* a generic result loses its type. Readings are from the
v2.4.13 binary; `own = total − import baseline`, where the baseline is measured by
`baseline_import_collections.mw` (import only, no calls of our own) and is **1** line at
this version: `want=String got=u8 :: hashmap_insert`, contributed by std's own code.

| Case | Situation | Today (own) | Target |
|---|---|---|---|
| `case14_generic_result_string_ok` | `want_string(id::<String>("hi"))` | **1** — `want=String got=int` | 0 |
| `case15_generic_result_int_ok` | `want_int(id::<i64>(7))` | 0 | 0 (must stay 0) |
| `case16_accessor_string_ok` | `want_string(vec_get(&v, 0))` over `Vec<String>` | **1** — `want=String got=int` | 0 |
| `case17_accessor_int_ok` | `want_int(vec_get(&v, 0))` over `Vec<i64>` | 0 | 0 (must stay 0) |
| `case18_insertion_string_key_ok` | `hashmap_insert(&mut m, "key", 1)` | **0** | 0 |
| `case19_incompatible_must_report` | `want_string(vec_get(&v, 0))` over `Vec<i64>` | 1 — `want=String got=int` | 1 (must stay 1) |

Cases 15 and 17 pass **by accident**: the placeholder kind (`int`) happens to match the
expected `i64`. They exist so a fix cannot break them. Cases 14/16 are known false
positives; case 19 is the negative that forbids "stop reporting generic results" as a fix.

## Where the type is lost — one shared point, two very different cases

The call-result type is produced in exactly one place per callee kind:

```
compiler/typeck.mw:737   let ret_sym = target_fn.ret_ty_sym;
compiler/typeck.mw:738   return tc_resolve_type(tc, types, struct_defs, ret_sym);
compiler/typeck.mw:745   let ret_sym = ext.ret_ty_sym;          (extern functions)
```

`tc_resolve_type` knows type aliases and struct names, nothing else: a return type of `T`
is neither, so it becomes `ty_int`. Every generic-returning callee — `id`, `vec_get`,
`hashmap_get` — collapses there. That is the shared point; no `vec_get`/`hashmap_insert`
special case is needed or wanted.

But the two cases reach it with different information available:

- **case14 (`id::<String>`)** — the instantiation is *written at the call site* in the
  turbofish, which the parser already stores in `expr.int_val`. A substitution at line 737
  can use it, with no new state. **Fixable now.**
- **case16 (`vec_get`)** — there is no turbofish; `T` is only knowable from the container
  argument (`&mut Vec<T>` → element type). `TcType` does not carry element types for struct
  instantiations, so this cannot be recovered at line 737 at all. A special case would have
  to invent a table of "functions whose result is their first argument's element" — which
  is exactly the kind of rule this project has been removing. **Needs element-type
  tracking, not a lookup.**

