# Game plan: `Vec` / `Def::List`

The last structural gap. Written 2026-09-02, against `2112440` (37 tests green).
Temporary — this file dies with the spike repo when it folds back into
`crates/formoxus`.

## The facet API (verified, not recalled)

**Detect:** `shape.def` matches `Def::List(ListDef)`, and `list_def.t` is the
element shape — exactly parallel to `Def::Option`'s `option_def.t`.

**Read:** `Peek::into_list()` → `PeekList`, with `.len()`, `.get(i)`, `.iter()`.

**Build** (from facet's own `tests/partial/list_leak.rs`):

```rust
Partial::alloc::<Vec<i32>>()?
    .init_list()?
    .begin_list_item()?.set(10)?.end()?
    .begin_list_item()?.set(20)?.end()?
    .build()?
```

`begin_list_item()` pushes a frame; `end()` pops it. So for a `Vec` **field**:

```
begin_field("answer_choices")?   // lands on the Vec slot
init_list()?
  per row:  begin_list_item()? → <write the element> → end()?
end()?                           // pops begin_field
```

## The hard question: how many rows?

The count is not in the shape. This is the **exact analog of "which variant"** —
and it should get the same answer, for consistency with the design we already
committed to: *the shape of the form is fully determined before data entry.*

So **row count is a construction parameter**, alongside variant choices. That
suggests generalizing rather than bolting on a parallel mechanism:

- `MissingVariants` becomes something like `MissingChoices`, carrying both
  unanswered variant paths **and** unanswered list-length paths.
- `missing_variants` becomes `missing_choices`, reporting both.

**The subtle part:** the two interact. A `Vec<SomeEnum>` needs one variant
choice *per row*, so you can't know how many variant questions to ask until the
length is answered. That's just another turn of the disclosure loop already
built — length answered → rows appear → each row's enum becomes a new question.
The existing loop handles this if `missing_choices` descends into list elements
*after* the length is known, exactly as it descends into a variant's fields
after the variant is known.

**Add/remove a row without reactivity.** Worth noting this works today, given
the uncontrolled design: "add a row" is a discrete action, not a keystroke.
Collect the current DOM values, rebuild with `length + 1`, `apply()` the
collected values back. Nothing is lost, no signals needed. The same trick
covers remove. This is why locking the count up front is much less restrictive
than it first sounds — unlike the enum variant, changing it later is cheap and
lossless, because row *n*'s paths don't move when row *n+1* appears.

## Structure: rows are members named by index

A new member kind — `ListSet { name, label, rows: Vec<Box<dyn FormMember>> }` —
where each row's `name()` is its index as a string (`"0"`, `"1"`, …).

That makes the agreed path convention fall out of the existing `qualify`
machinery with **no special casing**:

| model | row member | resulting leaf path |
|---|---|---|
| `Vec<String>` | `FormField<String>` named `"0"` | `correct_answers.0` |
| `Vec<AnswerChoice>` | `FieldSet` named `"0"` | `answer_choices.0.text` |

`collect_leaves`/`apply_leaves` then need no changes at all — they already
qualify a container's name onto each member's.

## The one real refactor: split "position" from "write"

`ListSet::write_into` can't just call each row's `write_into`, because every
member kind starts with `begin_field(&self.name)` — but inside a list item
there is no named field; the item *is* the position. Calling
`begin_field("0")` would be wrong.

Fix: split the trait method in two.

```rust
/// Write my value at the CURRENT position.
fn write_value_into<'p>(&self, p: Partial<'p>) -> Result<Partial<'p>, ReflectError>;

/// Descend into my own field, write, come back. Default impl:
fn write_into<'p>(&self, p: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
    self.write_value_into(p.begin_field(&self.name())?)?.end()
}
```

Then `ListSet` uses `begin_list_item()? → row.write_value_into(..)? → end()?`,
and every existing member keeps working through the default `write_into`. This
is better factoring regardless of `Vec` — it separates "where am I" from "what
am I," which is exactly the distinction `begin_list_item` forces.

Watch the interaction with `VariantSet`'s `optional` flag: its `begin_some()`
belongs in `write_value_into` (it's part of writing the value), not in the
positioning half.

## Suggested order

1. **`Vec<String>` only**, fixed length, edit mode. Smallest thing that
   exercises `init_list`/`begin_list_item` and the index-named rows. Prove
   `form_for(&value)` round-trips.
2. **The `write_value_into` split**, as its own step — it touches every member
   kind, so keep it separate from behavioral change and let the 37 existing
   tests confirm nothing moved.
3. **`Vec<Struct>`** — should mostly work once 1 and 2 land, since a row is
   just a `FieldSet`. Confirms nested paths (`answer_choices.0.text`).
4. **Create mode + lengths** — generalize `missing_variants` → `missing_choices`
   with the length questions. This is the biggest design step; do it last, when
   the mechanics underneath are already proven.
5. **`Vec<Enum>`** — the interaction case: length answered reveals per-row
   variant questions. One test proving the loop terminates.
6. **Then `Question`** — the actual motivating case, which is
   `Vec<AnswerChoice>` + `Vec<String>` + an enum, i.e. all of the above at once.

## Known adjacent gap (not blocking)

Bare top-level enum model (`form_for::<SomeEnum>`) still fails with `"must
select variant before selecting enum fields"`. Parked deliberately — `Question`
never has an enum as the top-level model. A top-level `Vec` model would likely
have the same shape of problem, and can be parked for the same reason.
