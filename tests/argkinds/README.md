# ARGKINDS corpus — validating the instrument before trusting its counts

`forge argkinds <file>` prints one line per call argument whose declared parameter kind
differs from the inferred argument kind:

```
ARGKIND want=<kind> got=<kind> :: <callee>
```

It never rejects anything. Because a report can reach zero for the wrong reason (an
inference system that is simply permissive), these five cases pin what the instrument
must and must not say. They are checked by hand for now; the readings below are from
the released binary of the version noted.

| Case | File | Must | Reading (v2.4.11) |
|---|---|---|---|
| 1 | `case1_ref_to_struct.mw` — `&p` passed to a by-value `Probe` | **not** reported (implicit deref) | no line ✅ |
| 2 | `case2_string_to_int.mw` — `"hello"` to an `i64` parameter | **reported** | `want=int got=String` ✅ |
| 3 | `case3_generic_string.mw` — `id::<String>("hi")` | **not** reported: substitution makes the expectation `String`, which matches | no line ✅ (was `want=int` before substitution) |
| 3b | `case3b_generic_turbofish_mismatch.mw` — `id::<i64>("hi")` | **reported** as `want=int`, proving substitution produced the expectation rather than silencing the check | `want=int got=String` ✅ |
| 4 | `case4_nested_ref.mw` — `&&Probe` to a by-value `Probe` | **reported** (one level of deref only) | `want=struct got=ref` ✅ |
| 5 | `case5_ref_to_scalar.mw` — `&i64` to an `i64` parameter | **not** reported (not a struct special case) | no line ✅ |

Case 3 and 3b are deliberately a pair: 3 could be satisfied by simply *skipping* generic
parameters, which would be a lie. 3b is what makes 3 meaningful — an instantiated parameter
whose declared kind is `int` must still catch a `String` argument.

## The rule this pins

A by-value parameter accepts `&x` / `&mut x`, so the comparison unwraps **one** level and
**only** when the parameter is not itself a reference. `&&T` against `T` therefore stays
reported. When a parameter *is* the callee's generic parameter, the expectation comes from
the turbofish instantiation; with no turbofish the argument is left **unreported** rather
than compared against the placeholder kind. The rule lives in the argument-kind report
only: `ty_ref` still carries its referent (`left`), and no other checker rule sees the
unwrapping.

## Known limits of the model, not of the rule

- **Mutability is not represented.** `&T` and `&mut T` both become `ty_ref`, so nothing
  here can distinguish them, and nothing should pretend to. Distinguishing them is a
  prerequisite for any *enforced* rule that talks about mutability.
- **The comparison is kind-level, not type-level.** `struct` vs `struct` never clashes,
  so `Vec` passed where `HashMap` is expected is not reported. Tightening this needs the
  `sym` comparison that only makes sense after substitution.
- **Generic parameters resolve to `int`** (case 3's `want=`), which is the substitution
  work, not the deref rule.
