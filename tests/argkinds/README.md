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
