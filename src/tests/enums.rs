//! Enum fields, optional enums, and the iterative disclosure loop.

use crate::*;
use facet::Facet;
use std::collections::HashMap;
use super::models::{Location, Mode, Shape};

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Drawing {
    pub name: String,
    pub shape: Shape,
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Config {
    pub shape: Shape,
    pub mode: Mode,
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Outer {
    pub title: String,
    pub drawing: Drawing,
}

// An enum reachable only *through* another enum's variant — the shape that
// makes variant discovery iterative rather than one-shot.
#[derive(Facet, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Inner {
    A { x: f64 },
    B { y: f64 },
}

#[derive(Facet, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Outer2 {
    First { inner: Inner },
    Second { n: u32 },
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Doc {
    pub outer: Outer2,
}

/// An enum behind an `Option` — the case nothing covered until now.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Sketch {
    pub name: String,
    pub shape: Option<Shape>,
}

/// Expected `missing_variants` output for enums NOT behind an `Option`.
fn variants(pairs: &[(&str, &[&str])]) -> HashMap<String, VariantOptions> {
    opt_variants(pairs, false)
}

/// Same, for enums that ARE behind an `Option` — `Absent` is legal, so a
/// picker should offer a "none" entry alongside the variants.
fn optional_variants(pairs: &[(&str, &[&str])]) -> HashMap<String, VariantOptions> {
    opt_variants(pairs, true)
}

fn opt_variants(pairs: &[(&str, &[&str])], optional: bool) -> HashMap<String, VariantOptions> {
    pairs
        .iter()
        .map(|(path, vs)| {
            (
                path.to_string(),
                VariantOptions {
                    optional,
                    variants: vs.iter().map(|v| v.to_string()).collect(),
                },
            )
        })
        .collect()
}

/// Shorthand for building a choice map.
fn chose(pairs: &[(&str, VariantChoice)]) -> HashMap<String, VariantChoice> {
    pairs
        .iter()
        .map(|(path, c)| (path.to_string(), c.clone()))
        .collect()
}

fn named(name: &str) -> VariantChoice {
    VariantChoice::Named(name.to_string())
}

// A struct with no enum fields anywhere has nothing to choose — the empty
// map is exactly what makes `empty_form::<T>().expect("no enum fields, so nothing to choose")` safe for such a `T`.
#[test]
fn no_enum_fields_yields_empty_map() {
    assert_eq!(required_variants::<Location>(), HashMap::new());
}

// One enum field: keyed by the field name, listing that enum's variants in
// declaration order (Circle before Rectangle).
#[test]
fn single_enum_field_lists_its_variants_in_declaration_order() {
    assert_eq!(
        required_variants::<Drawing>(),
        variants(&[("shape", &["Circle", "Rectangle"])]),
    );
}

// Two enum fields → two entries; `mode`'s unit variants still enumerate.
#[test]
fn multiple_enum_fields_including_unit_variants() {
    assert_eq!(
        required_variants::<Config>(),
        variants(&[
            ("shape", &["Circle", "Rectangle"]),
            ("mode", &["Fast", "Slow"]),
        ]),
    );
}

// An enum nested under a struct field is keyed by its qualified path —
// `drawing.shape`, never `shape` — the same field-name qualification
// `collect_leaves` uses, so two nested enums can't collide.
#[test]
fn nested_enum_field_is_qualified_by_path() {
    assert_eq!(
        required_variants::<Outer>(),
        variants(&[("drawing.shape", &["Circle", "Rectangle"])]),
    );
}

// ── Discovery: optionality and the legality of Absent ──

// An enum behind an `Option` is reported as optional, so a picker knows to
// offer a "none" entry alongside the variants.
#[test]
fn optional_enum_is_discovered_as_optional() {
    assert_eq!(
        required_variants::<Sketch>(),
        optional_variants(&[("shape", &["Circle", "Rectangle"])]),
    );
}

// Choosing Absent answers an optional enum completely — nothing is left.
#[test]
fn absent_satisfies_an_optional_enum() {
    let chosen = chose(&[("shape", VariantChoice::Absent)]);
    assert_eq!(missing_variants::<Sketch>(&chosen), HashMap::new());
}

// But Absent is not a real answer for an enum that isn't behind an Option:
// it stays reported, so the caller sees the options it actually has.
#[test]
fn absent_is_rejected_where_the_enum_is_not_optional() {
    let chosen = chose(&[("shape", VariantChoice::Absent)]);
    assert_eq!(
        missing_variants::<Drawing>(&chosen),
        variants(&[("shape", &["Circle", "Rectangle"])]),
    );
}

// Naming a variant this enum doesn't have is treated the same as not
// answering — which hands back the real options instead of failing later
// inside construction.
#[test]
fn an_unknown_variant_name_reports_the_real_options() {
    let chosen = chose(&[("shape", named("Triangle"))]);
    assert_eq!(
        missing_variants::<Drawing>(&chosen),
        variants(&[("shape", &["Circle", "Rectangle"])]),
    );
}

// ── Option<Enum>: currently broken, fixed by the Absent/begin_some work ──

// KNOWN FAILING (deliberate). `member_for` unwraps the `Option` and builds a
// `VariantSet` whose `write_into` does `begin_field("shape")` — landing on the
// `Option<Shape>` slot, not the `Shape` inside it — and then asks it for a
// variant named "Circle". facet models `Option` as an enum over `None`/`Some`,
// so that lookup can't succeed. The fix is `begin_some()` before selecting.
#[test]
fn edit_mode_round_trips_an_optional_enum() {
    let sketch = Sketch {
        name: "Doodle".to_string(),
        shape: Some(Shape::Circle { radius: 1.5 }),
    };
    let mut form = form_for(&sketch);
    assert_eq!(form.validate(), Some(sketch));
}

// The other half: `None` should round-trip to `None`, with no leaves under
// `shape` at all.
#[test]
fn edit_mode_round_trips_an_absent_optional_enum() {
    let sketch = Sketch {
        name: "Doodle".to_string(),
        shape: None,
    };
    let mut form = form_for(&sketch);
    assert_eq!(form.validate(), Some(sketch));
}

// Create mode, Absent: no leaves under `shape` at all, and validate()
// produces `None` for it.
#[test]
fn create_mode_absent_builds_a_none() {
    let chosen = chose(&[("shape", VariantChoice::Absent)]);
    let mut form = empty_form_with_variants::<Sketch>(&chosen)
        .expect("Absent is a legal answer for an optional enum");

    let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, vec!["name"], "Absent contributes no leaves");

    form.apply_form_values(&[("name".to_string(), "Doodle".to_string())]);
    assert_eq!(
        form.validate(),
        Some(Sketch {
            name: "Doodle".to_string(),
            shape: None,
        }),
    );
}

// Create mode, a chosen variant behind an Option: its fields appear under
// the enum's path, and validate() wraps the result in `Some`.
#[test]
fn create_mode_named_behind_an_option_builds_a_some() {
    let chosen = chose(&[("shape", named("Circle"))]);
    let mut form = empty_form_with_variants::<Sketch>(&chosen)
        .expect("Circle is a variant of Shape");

    form.apply_form_values(&[
        ("name".to_string(), "Doodle".to_string()),
        ("shape.radius".to_string(), "2.5".to_string()),
    ]);
    assert_eq!(
        form.validate(),
        Some(Sketch {
            name: "Doodle".to_string(),
            shape: Some(Shape::Circle { radius: 2.5 }),
        }),
    );
}

// The absent field is visible but inert, so the user can see they left it
// out. Disabled means the browser won't submit it, so ABSENT_DISPLAY never
// comes back as a value.
#[test]
fn absent_renders_a_disabled_placeholder() {
    let sketch = Sketch {
        name: "Doodle".to_string(),
        shape: None,
    };
    let html = form_for(&sketch).render();
    assert!(html.contains(ABSENT_DISPLAY), "html: {html}");
    assert!(html.contains("disabled"), "html: {html}");
}

// ── Iterative disclosure: choices reveal further choices ──

// The whole loop in one test. `outer.inner` does not exist as a question
// until `outer` is answered with the variant that contains it — which is
// why a single up-front walk could never have found it.
#[test]
fn choosing_a_variant_reveals_the_enums_inside_it() {
    // Nothing chosen: only the outer enum is reachable.
    assert_eq!(
        missing_variants::<Doc>(&chose(&[])),
        variants(&[("outer", &["First", "Second"])]),
        "outer.inner must be invisible before outer is answered",
    );

    // Answering `outer` with the variant that holds an enum reveals it.
    assert_eq!(
        missing_variants::<Doc>(&chose(&[("outer", named("First"))])),
        variants(&[("outer.inner", &["A", "B"])]),
    );

    // The other branch holds no enum, so answering it finishes the loop.
    assert_eq!(
        missing_variants::<Doc>(&chose(&[("outer", named("Second"))])),
        HashMap::new(),
    );

    // Answering the revealed question finishes the first branch too.
    assert_eq!(
        missing_variants::<Doc>(&chose(&[
            ("outer", named("First")),
            ("outer.inner", named("A")),
        ])),
        HashMap::new(),
    );
}

// The two walks agree: once `missing_variants` is empty, construction
// succeeds — that's the invariant keeping the `Result` from lying.
#[test]
fn a_fully_answered_nested_enum_builds_and_round_trips() {
    let chosen = chose(&[("outer", named("First")), ("outer.inner", named("A"))]);
    assert_eq!(missing_variants::<Doc>(&chosen), HashMap::new());

    let mut form =
        empty_form_with_variants::<Doc>(&chosen).expect("every reachable enum is answered");

    // The doubly-nested leaf is qualified the whole way down.
    let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, vec!["outer.inner.x"]);

    form.apply_form_values(&[("outer.inner.x".to_string(), "1.25".to_string())]);
    assert_eq!(
        form.validate(),
        Some(Doc {
            outer: Outer2::First {
                inner: Inner::A { x: 1.25 },
            },
        }),
    );
}

// Partial answers stay errors, and the error names what is still open —
// which is what a caller renders as the next round of pickers.
#[test]
fn a_half_answered_nested_enum_reports_only_what_is_still_open() {
    let chosen = chose(&[("outer", named("First"))]);
    let err = empty_form_with_variants::<Doc>(&chosen)
        .expect_err("outer.inner is revealed but unanswered");
    assert_eq!(err.0, variants(&[("outer.inner", &["A", "B"])]));
}

// ── Construction: the enum field actually round-trips through validate() ──

// Create mode: the caller picks the variant, the chosen variant's fields
// become leaves under the enum's path, and validate() builds that variant.
#[test]
fn empty_form_with_variants_builds_and_validates_the_chosen_variant() {
    let chosen = chose(&[("shape", named("Circle"))]);
    let mut form = empty_form_with_variants::<Drawing>(&chosen).expect("every enum has a chosen variant");

    // Only the chosen variant's field is present, keyed under `shape`.
    let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
    assert!(paths.contains(&"name".to_string()), "paths: {paths:?}");
    assert!(paths.contains(&"shape.radius".to_string()), "paths: {paths:?}");

    form.apply_form_values(&[
        ("name".to_string(), "My Drawing".to_string()),
        ("shape.radius".to_string(), "3.5".to_string()),
    ]);
    assert_eq!(
        form.validate(),
        Some(Drawing {
            name: "My Drawing".to_string(),
            shape: Shape::Circle { radius: 3.5 },
        }),
    );
}

// Edit mode: the value pins the variant (no map needed) and populates its
// fields; validate() replays the same variant via select_variant_named.
#[test]
fn form_for_round_trips_an_enum_field() {
    let drawing = Drawing {
        name: "Rect".to_string(),
        shape: Shape::Rectangle {
            width: 2.0,
            height: 4.0,
        },
    };
    let mut form = form_for(&drawing);
    assert_eq!(form.validate(), Some(drawing));
}

// The enum lives one struct deep — exercises the qualified path
// (`drawing.shape.…`) through both construction and write_into.
#[test]
fn nested_enum_field_round_trips() {
    let outer = Outer {
        title: "T".to_string(),
        drawing: Drawing {
            name: "N".to_string(),
            shape: Shape::Circle { radius: 1.0 },
        },
    };
    let mut form = form_for(&outer);
    assert_eq!(form.validate(), Some(outer));
}

// OPEN QUESTIONS — deliberately not asserted, because the behavior isn't
// decided yet. Flagging so we choose on purpose rather than by accident:
//
//   1. `Option<SomeEnum>` field — is it in the map (needs a variant IF
//      present) or omitted (absence is a legal, choice-free state)?
//   2. A top-level enum `T` itself (`required_variants::<Shape>()`) — what's
//      the key, `""`? Or is a bare-enum model simply out of scope?
//   3. An enum nested *inside a variant's* fields — does the walk recurse
//      through variants, and if so what are those paths (they only exist
//      once a parent variant is chosen)?
