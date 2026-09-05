# Plan: create mode, reactive rows, and variants-as-data

Supersedes the original `Vec`/`Def::List` plan (written 2026-09-02), whose steps
1–3 are done. Rewritten 2026-09-05 against `786b899` (68 tests green).
Temporary — this file dies with the spike when it folds into `crates/formoxus`.

## Where things stand

Done: edit-mode lists (rows named by index, `write_value_into` split), nested
`Vec<Vec<T>>`, `Vec<Enum>`, and optional containers (`OptionMember` peeling one
`Option` layer per recursion). What is left is **create mode** — and Todd's
reframe below turns that from "a third construction question" into something
smaller and more uniform.

## The reframe: the DOM is the source of truth for STRUCTURE, not just values

Today, names are derived from structure, one way: the shape decides the paths,
`collect_leaves` emits them, `apply_leaves` looks them up. The uncontrolled
design already trusts the DOM for *values* on submit.

The observation is that the naming convention is **lossless**, so structure is
recoverable from names too. `answer_choices.0.text` says "field `text`, of row 0,
of list `answer_choices`". So on submit we can walk `T::SHAPE` and, at each list,
count the `prefix.N` keys present rather than being told the length up front.

That kills the "row count is a construction parameter" premise of the old plan.

### Lengths

- **Blank forms default to 1 row**, not 0. Zero rows gives the user nothing to
  type into and no hint the list exists. (leptos_form reaches the same place from
  the other direction: `VecConfigSize::Bounded { min: Some(1) }` pads up to the
  minimum at render.)
- A caller may still pass an initial count, but it is a **render hint, not a
  structural commitment** — nothing downstream depends on it being right.
- **Sparse on the wire, compacted on read.** Deleting the middle of three rows
  leaves `answers.0`, `answers.2`; renumbering live DOM is the fragile thing we
  avoid. Count the distinct indices present, take them in order, rebuild densely.
  Growth is index-stable (row *n*'s paths never move when *n+1* appears), which
  is what makes a purely-DOM "add row" safe in the first place.

### Variants (Todd's idea, and the bigger half)

A **reactive `<select>` per enum field**: pick a variant, that subtree's fields
appear. The choice is then written into the DOM under a marker segment that
**cannot collide with a Rust field name**, e.g.

```
data.$variant = "MultipleChoice"
data.answer_choices.0.text = "..."
```

`$` is not valid in a Rust identifier and is fine in an HTML `name`, so there is
no sentinel problem — this is the same trick that already lets integer segments
(`answers.0`) coexist with field names. For an `Option<Enum>` left empty, the
select's value is `""`, reusing "`""` IS absence" rather than inventing a second
convention.

## What this retires

If the variant round-trips as data, it stops being a construction parameter, and
the whole apparatus built around *asking* for it goes away:

- `MissingVariants`, `VariantOptions`, `missing_variants`, `required_variants`
- the iterative disclosure loop
- **the invariant that has bitten us twice**: "pre-flight empty ⟺ construction
  succeeds", which required the pre-flight walk and the construction walk to
  agree about what to descend into

`VariantChoice` survives as the *state* a `VariantSet` holds; it just arrives
from the DOM instead of from the caller.

The constructor set collapses to three, one per genuine mode:

```rust
form_for(&value)                    -> Form<T>                    // edit; infallible
blank_form::<T>()                   -> Form<T>                    // initial render; infallible
form_from_values::<T>(&values)      -> Result<Form<T>, FormDataError>  // rebuild from a submission
```

`empty_form` / `empty_form_with_variants` both disappear. Note `blank_form`
becomes **infallible** — the reason it returned a `Result` was precisely the
unchosen-variant problem.

## Reading back is fallible — and that is new

`apply` is infallible today: unknown keys are ignored, bad values become
`FieldValue::Invalid` and stay attached to their field. That works because
structure is fixed and only *values* come from the wire.

Once structure comes from the wire, there are failures with **no field to attach
themselves to**. Todd's point, and it needs its own error type:

```rust
pub enum FormDataError {
    /// `data.$variant = "Nonsense"` — not a variant of that enum.
    UnknownVariant { path: String, found: String, expected: Vec<String> },
    /// Fields present under an enum path, but no `$variant` marker.
    MissingVariant { path: String },
    /// `$variant` empty on an enum that is NOT behind an `Option`.
    IllegalAbsence { path: String },
    /// A path segment where an index was expected but wasn't an integer.
    MalformedPath { path: String },
}
```

Collect these rather than failing fast, matching how field errors already
accumulate in place rather than short-circuiting.

Worth being explicit that this is a **trust boundary**: submitted names are now
structural instructions, so every one of these is reachable from a hostile or
merely stale client, not just from our own bugs.

## Does this reverse the "lock the variant" decision?

Partly, deliberately, and the original reasoning does not forbid it.

That decision (2026-09-02) had two parts. The first — *"no blank form before a
variant is picked"* — was justified by "it eliminates the reactivity
requirement." Reactivity for the enum select is now accepted, so that
justification is spent, and the UX it was protecting (don't let users type into a
form whose shape is undecided) is preserved anyway: with no variant chosen, the
subtree renders as just the select.

The second — *"no changing the variant once displayed"* — was justified by data
loss being ill-posed (a half-filled `TrueFalse` has no coherent `MultipleChoice`
representation). **That argument still stands**, and it now becomes an explicit UX
rule rather than a structural guarantee: changing the select **wipes that
subtree**, and the user should be warned. Adding a *new* row to a `Vec<Enum>` is
not a change — it is a new sub-form, and "add question → which type?" is a
natural flow.

## Add/remove a row, by kind

- `Vec<Scalar>` / `Vec<Struct>` — **pure DOM**. Clone a row template, bump the
  index. No Rust round trip.
- `Vec<Enum>` — needs a Rust rebuild, because a new row's members don't exist
  until its variant is answered. That is the collect → rebuild → re-apply trick,
  on a discrete action, which is exactly what it is for.

## Open questions

1. **Infer the row count, or emit an explicit hidden `answers.$len`?** Django
   refuses to infer and ships a `TOTAL_FORMS` counter. Inference needs no extra
   name and matches "the DOM is the truth on submit"; an explicit count is
   unambiguous against truncated or hostile submissions. Currently leaning
   inference, but this is the one worth prototyping both ways.
2. **Does `$variant` become a leaf?** If it shows up in `collect_leaves`, then a
   chosen fieldless variant finally *does* have a leaf — which would have
   prevented the `is_present` data-loss bug. Keep the per-kind `is_present`
   regardless (it is more honest), but the interaction is worth understanding
   rather than tripping over.
3. **`OptionMember` and `$variant` both express absence.** For `Option<Enum>`,
   an empty select and "no leaves under here" must not disagree.
4. **Does `Form<T>` still need `T` at all** once construction is data-driven?
   Probably yes — `Partial::alloc::<T>()`/`materialize::<T>()` still need it.

## Suggested order

1. **Lengths first, no reactivity.** `list_member` builds N rows in `Blank` mode,
   defaulting to 1. Keeps everything green and is a prerequisite for the rest.
2. **Count rows from submitted names** — `form_from_values`, sparse→compacted,
   with `Vec<Scalar>` and `Vec<Struct>` tests. Still no enums, so still
   infallible in practice.
3. **`$variant` in the DOM**, read side first: `form_from_values` learns to read
   it, and `FormDataError` appears. This is where `MissingVariants` and the
   disclosure loop can be deleted, with `enum_tests` as the net.
4. **The reactive select**, widget layer only. `Form`/`FormMember` stay plain
   data — signals live at the widget boundary, per the standing decision.
5. **`Question`** — the actual motivating case, now with no construction
   questions at all.
