//! A runtime-reflection alternative to formoxus's derive macros: build a form
//! from a model's `Facet` shape instead of from a hand-written form struct.

pub mod build;
pub mod choices;
pub mod error;
pub mod fields;
pub mod form;
pub mod members;

// A flat root, so `use facet_form_spike::*` (and the test modules' `use super::*`)
// reach the whole vocabulary without knowing which module each name lives in.
pub use choices::{MissingVariants, VariantChoice, VariantOptions};
pub use error::{FieldError, FormError};
pub use fields::{FieldValue, FormField};
pub use form::{Form, empty_form, empty_form_with_variants, form_for};
pub use members::{FieldSet, FormMember, ListSet, VariantSet};
// The iterative-disclosure API: ask what still needs choosing, render pickers, repeat.
pub use choices::{missing_variants, required_variants};

// The one crate-internal item the test modules reach for directly.
#[cfg(test)]
pub(crate) use members::ABSENT_DISPLAY;

// The test modules `use super::*` and inherit these, exactly as they did when
// every item lived in this one file. Child modules can see a parent's private
// imports, so this keeps the test code unchanged by the split.
#[cfg(test)]
use facet::Facet;
#[cfg(test)]
use std::{collections::HashMap, fmt::Debug, marker::PhantomData};

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Event {
        pub id: u32, // server-assigned — not collected by any form field
        pub title: String,
        pub location: Location,
    }

    /// What a `Form<T>` actually validates into: every field here is genuinely
    /// collected by some `FormMember`, so `Partial::build()` never hits an
    /// uninitialized field. Surreal assigns `id` on create; on edit, the caller
    /// re-attaches the `id` it already had from the original fetch — `Form`
    /// itself never needs to know about it.
    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct EventForCreate {
        pub title: String,
        pub location: Location,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Location {
        pub street: String,
        pub city: String,
        pub zip: String,
    }

    fn text_field(name: &str, required: bool, value: FieldValue<String>) -> Box<dyn FormMember> {
        Box::new(FormField {
            name: name.to_string(),
            label: None,
            required,
            value,
            errors: Vec::new(),
        })
    }

    fn location_members(
        street: FieldValue<String>,
        city: FieldValue<String>,
        zip: FieldValue<String>,
    ) -> Vec<Box<dyn FormMember>> {
        vec![
            text_field("street", true, street),
            text_field("city", true, city),
            text_field("zip", true, zip),
        ]
    }

    fn location_field_set(
        street: FieldValue<String>,
        city: FieldValue<String>,
        zip: FieldValue<String>,
    ) -> Box<dyn FormMember> {
        Box::new(FieldSet {
            name: "location".to_string(),
            label: Some("Location".to_string()),
            members: location_members(street, city, zip),
            errors: Vec::new(),
        })
    }

    /// Has an `Option` field, which none of the other models do — that's the
    /// path `form_for` uses to decide `required`.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Rsvp {
        name: String,
        guests: u32,
        note: Option<String>,
    }

    /// The same struct type twice in one form — the case that decides whether
    /// leaf paths can be keyed by struct name.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Trip {
        origin: Location,
        destination: Location,
    }

    fn member_names(members: &[Box<dyn FormMember>]) -> Vec<String> {
        members.iter().map(|m| m.name()).collect()
    }

    #[test]
    fn repeated_struct_types_get_distinct_paths() {
        let form = empty_form::<Trip>().expect("no enum fields, so nothing to choose");
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();

        assert_eq!(
            paths,
            vec![
                "origin.street",
                "origin.city",
                "origin.zip",
                "destination.street",
                "destination.city",
                "destination.zip",
            ]
        );
    }

    #[test]
    fn form_for_none_walks_the_shape_into_empty_members() {
        let form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");

        assert_eq!(member_names(&form.members), vec!["title", "location"]);

        // The nested struct field became a FieldSet with its own members,
        // discovered purely from `Location`'s shape.
        let rendered = form.render();
        for name in ["title", "street", "city", "zip"] {
            assert!(
                rendered.contains(&format!(r#"name="{name}""#)),
                "expected an input for {name} in:\n{rendered}"
            );
        }
        // Nothing was seeded, so every input is blank.
        assert!(!rendered.contains(r#"value="Board Game Night""#));
    }

    #[test]
    fn form_for_none_is_invalid_until_filled() {
        let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
        assert_eq!(form.validate(), None);
        assert!(form.has_errors());
    }

    #[test]
    fn form_for_some_round_trips_the_model() {
        let event = EventForCreate {
            title: "Board Game Night".to_string(),
            location: Location {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
                zip: "12345".to_string(),
            },
        };

        let mut form = form_for(&event);
        assert!(!form.has_errors());
        assert_eq!(form.validate(), Some(event));
    }

    #[test]
    fn option_fields_are_not_required() {
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");

        // `note: Option<String>` is optional, so an empty form only complains
        // about `name` and `guests`.
        for m in form.members.iter_mut() {
            m.validate();
        }
        let complaining: Vec<String> = form
            .members
            .iter()
            .filter(|m| m.has_errors())
            .map(|m| m.name())
            .collect();
        assert_eq!(complaining, vec!["name", "guests"]);
    }

    #[test]
    fn option_fields_round_trip_both_ways() {
        let with_note = Rsvp {
            name: "Ada".to_string(),
            guests: 2,
            note: Some("bringing dessert".to_string()),
        };
        assert_eq!(
            form_for(&with_note).validate(),
            Some(with_note)
        );

        let without_note = Rsvp {
            name: "Ada".to_string(),
            guests: 2,
            note: None,
        };
        assert_eq!(
            form_for(&without_note).validate(),
            Some(without_note)
        );
    }

    fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn applying_widget_values_round_trips_to_a_model() {
        // The full loop: shape-walk an empty form, take raw strings back in
        // the way a submit handler would, then validate into a model.
        let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
        form.apply(&values(&[
            ("title", "Board Game Night"),
            ("location.street", "123 Main St"),
            ("location.city", "Springfield"),
            ("location.zip", "12345"),
        ]));

        assert_eq!(
            form.validate(),
            Some(EventForCreate {
                title: "Board Game Night".to_string(),
                location: Location {
                    street: "123 Main St".to_string(),
                    city: "Springfield".to_string(),
                    zip: "12345".to_string(),
                },
            })
        );
    }

    #[test]
    fn non_string_scalars_parse_through_the_shape_vtable() {
        // `u32` here never touches `FromStr` — facet parses it from the shape.
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
        form.apply(&values(&[
            ("name", "Ada"),
            ("guests", "2"),
            ("note", "bringing dessert"),
        ]));

        assert_eq!(
            form.validate(),
            Some(Rsvp {
                name: "Ada".to_string(),
                guests: 2,
                note: Some("bringing dessert".to_string()),
            })
        );
    }

    #[test]
    fn unparseable_input_becomes_invalid_not_a_panic() {
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
        form.apply(&values(&[("name", "Ada"), ("guests", "not a number")]));

        assert_eq!(form.validate(), None);
        assert!(form.has_errors());

        // The bad input is preserved so the widget can show it back.
        let guests = form
            .leaves()
            .into_iter()
            .find(|(p, _)| p == "guests")
            .map(|(_, raw)| raw);
        assert_eq!(guests, Some("not a number".to_string()));
    }

    #[test]
    fn blanking_a_field_makes_it_empty_again() {
        let mut form = form_for(&Rsvp {
            name: "Ada".to_string(),
            guests: 2,
            note: Some("bringing dessert".to_string()),
        });
        // Clearing an optional field is legal; clearing a required one isn't.
        form.apply(&values(&[("note", ""), ("name", "")]));

        assert_eq!(form.validate(), None);
        let complaining: Vec<String> = form
            .members
            .iter()
            .filter(|m| m.has_errors())
            .map(|m| m.name())
            .collect();
        assert_eq!(complaining, vec!["name"]);
    }

    #[test]
    fn leaves_then_apply_is_an_identity_round_trip() {
        // The actual widget loop: seed a form from a model, hand the raw
        // strings to the widget layer, take them straight back, and validate.
        // Nothing edited in between, so this must land on the same model.
        let rsvp = Rsvp {
            name: "Ada".to_string(),
            guests: 2,
            note: Some("bringing dessert".to_string()),
        };

        let form = form_for(&rsvp);
        let round_tripped: HashMap<String, String> = form.leaves().into_iter().collect();

        let mut reloaded = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
        reloaded.apply(&round_tripped);

        assert_eq!(reloaded.validate(), Some(rsvp));
    }

    #[test]
    fn empty_event_form_is_invalid() {
        let mut form: Form<Event> = Form {
            title: Some("New Event".to_string()),
            members: vec![
                text_field("title", true, FieldValue::Empty),
                location_field_set(FieldValue::Empty, FieldValue::Empty, FieldValue::Empty),
            ],
            errors: Vec::new(),
            _type: PhantomData,
        };

        assert_eq!(form.validate(), None);
        assert!(form.has_errors());
    }

    #[test]
    fn location_form_round_trips_to_model() {
        // `Location` has no uncollected fields, so this exercises the core
        // `FormField::write_into` -> `Partial::build` -> `materialize` path
        // with nothing else in the way.
        let mut form: Form<Location> = Form {
            title: None,
            members: location_members(
                FieldValue::Valid("123 Main St".to_string()),
                FieldValue::Valid("Springfield".to_string()),
                FieldValue::Valid("12345".to_string()),
            ),
            errors: Vec::new(),
            _type: PhantomData,
        };

        let model = form.validate().expect("all required fields are filled");
        assert_eq!(
            model,
            Location {
                street: "123 Main St".to_string(),
                city: "Springfield".to_string(),
                zip: "12345".to_string(),
            }
        );
    }

    #[test]
    fn event_for_create_form_round_trips_to_model() {
        let mut form: Form<EventForCreate> = Form {
            title: Some("New Event".to_string()),
            members: vec![
                text_field(
                    "title",
                    true,
                    FieldValue::Valid("Board Game Night".to_string()),
                ),
                location_field_set(
                    FieldValue::Valid("123 Main St".to_string()),
                    FieldValue::Valid("Springfield".to_string()),
                    FieldValue::Valid("12345".to_string()),
                ),
            ],
            errors: Vec::new(),
            _type: PhantomData,
        };

        assert!(!form.has_errors());
        let model = form.validate().expect("all required fields are filled");
        assert_eq!(model.title, "Board Game Night");
        assert_eq!(model.location.street, "123 Main St");
    }
}

#[cfg(test)]
mod widget_tests {
    use super::*;
    // `dioxus::prelude` exports its own `Location`, so ours needs an explicit
    // name to win the glob-import ambiguity.
    use super::tests::{EventForCreate, Location as ModelLocation};
    use dioxus::prelude::*;
    use std::collections::HashMap;

    /// One signal per leaf input, keyed by qualified path.
    ///
    /// The rules-of-hooks question this answers: the field count isn't known
    /// until runtime, so we can't call `use_signal` in a loop. But `use_hook`
    /// runs its initializer exactly once, inside a live runtime — so a single
    /// hook call can mint N signals, and the *hook count* stays 1 no matter
    /// how many fields the shape turned out to have.
    fn use_field_signals<T: Clone + Debug + PartialEq + Facet<'static>>(
        form: &Form<T>,
    ) -> HashMap<String, Signal<String>> {
        let leaves = form.leaves();
        use_hook(|| {
            leaves
                .into_iter()
                .map(|(path, raw)| (path, Signal::new(raw)))
                .collect()
        })
    }

    #[component]
    fn EventFormView() -> Element {
        let form = use_hook(|| {
            form_for(&EventForCreate {
                title: "Board Game Night".to_string(),
                location: ModelLocation {
                    street: "123 Main St".to_string(),
                    city: "Springfield".to_string(),
                    zip: "12345".to_string(),
                },
            })
        });
        let signals = use_field_signals(&form);

        // Stable order so the rendered output is deterministic.
        let mut paths: Vec<String> = signals.keys().cloned().collect();
        paths.sort();

        rsx! {
            form {
                for path in paths {
                    input {
                        r#type: "text",
                        name: "{path}",
                        value: "{signals[&path]}",
                    }
                }
            }
        }
    }

    /// No signals at all: every input is uncontrolled, named by its qualified
    /// path, and the browser holds the editing state. On submit, `values()`
    /// hands the whole form back and we shuffle it into a `Form<T>` once.
    #[component]
    fn UncontrolledEventForm() -> Element {
        let form = use_hook(|| empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose"));
        let leaves = form.leaves();

        rsx! {
            form {
                onsubmit: move |e: FormEvent| {
                    let values: Vec<(String, String)> = e
                        .values()
                        .into_iter()
                        .filter_map(|(name, v)| match v {
                            FormValue::Text(text) => Some((name, text)),
                            // File inputs are a separate story — there's no
                            // text for a scalar widget to parse.
                            FormValue::File(_) => None,
                        })
                        .collect();
                    let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
                    form.apply_form_values(&values);
                    let _model = form.validate();
                },
                for (path, raw) in leaves {
                    input { r#type: "text", name: "{path}", value: "{raw}" }
                }
                button { r#type: "submit", "Save" }
            }
        }
    }

    #[test]
    fn uncontrolled_inputs_are_named_by_qualified_path() {
        // `FormData::values()` keys off the `name` attribute, so these names
        // are the entire contract between the DOM and `apply_form_values`.
        let html = render_to_html(UncontrolledEventForm);
        for path in ["title", "location.street", "location.city", "location.zip"] {
            assert!(
                html.contains(&format!(r#"name="{path}""#)),
                "expected an input named {path} in:\n{html}"
            );
        }
    }

    #[test]
    fn submitted_values_shuffle_into_a_model() {
        // Exactly the shape `FormData::values()` produces, minus the DOM.
        let submitted = vec![
            ("title".to_string(), "Board Game Night".to_string()),
            ("location.street".to_string(), "123 Main St".to_string()),
            ("location.city".to_string(), "Springfield".to_string()),
            ("location.zip".to_string(), "12345".to_string()),
        ];

        let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
        form.apply_form_values(&submitted);

        assert_eq!(
            form.validate(),
            Some(EventForCreate {
                title: "Board Game Night".to_string(),
                location: ModelLocation {
                    street: "123 Main St".to_string(),
                    city: "Springfield".to_string(),
                    zip: "12345".to_string(),
                },
            })
        );
    }

    fn render_to_html(app: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[test]
    fn component_mints_one_signal_per_leaf() {
        let html = render_to_html(EventFormView);

        // Nested paths are qualified, so `location.street` can't collide with
        // a top-level `street` in some other field set.
        for path in ["title", "location.street", "location.city", "location.zip"] {
            assert!(
                html.contains(&format!(r#"name="{path}""#)),
                "expected an input named {path} in:\n{html}"
            );
        }
    }

    #[test]
    fn signals_are_seeded_from_the_model() {
        let html = render_to_html(EventFormView);
        assert!(
            html.contains("Board Game Night"),
            "expected the seeded title in:\n{html}"
        );
        assert!(
            html.contains("123 Main St"),
            "expected the seeded street in:\n{html}"
        );
    }
}

#[cfg(test)]
mod enum_tests {
    use super::*;
    use super::tests::Location;

    // Vec-free enums, per the agreed first target — isolates the enum work from
    // the still-unproven `Vec`/`Def::List` handling. `#[repr(u8)]` is required
    // for facet to derive on an enum (it needs the discriminant repr).
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Unit variants — to prove they still enumerate even with no fields.
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Mode {
        Fast,
        Slow,
    }

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

    // Edit mode: the value pins the variant (no map needed) and seeds its
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
}

/// `Vec` / `Def::List` — edit mode. Rows are members named by their index, so
/// every path convention here falls out of the same `qualify` nesting a struct's
/// fields use, with no list-specific code in `collect_leaves`/`apply_leaves`.
///
/// Everything below seeds from a value. Create mode is deliberately absent: the
/// row count isn't in the shape, so it's a construction parameter like an enum's
/// variant, and it isn't plumbed through yet (VEC_PLAN.md step 4).
#[cfg(test)]
mod vec_tests {
    use super::enum_tests::Shape;
    use super::tests::Location;
    use super::*;

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Quiz {
        title: String,
        answers: Vec<String>,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Venues {
        places: Vec<Location>,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Grid {
        rows: Vec<Vec<String>>,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Drawings {
        shapes: Vec<Shape>,
    }

    fn quiz() -> Quiz {
        Quiz {
            title: "Unit 1".to_string(),
            answers: vec!["alpha".to_string(), "beta".to_string()],
        }
    }

    fn venues() -> Venues {
        Venues {
            places: vec![
                Location {
                    street: "123 Main St".to_string(),
                    city: "Springfield".to_string(),
                    zip: "12345".to_string(),
                },
                Location {
                    street: "9 Elm".to_string(),
                    city: "Shelbyville".to_string(),
                    zip: "99999".to_string(),
                },
            ],
        }
    }

    #[test]
    fn scalar_rows_round_trip() {
        let mut form = form_for(&quiz());
        assert_eq!(form.validate(), Some(quiz()));
    }

    #[test]
    fn an_empty_list_round_trips() {
        // `init_list` with no `begin_list_item` at all — the degenerate case
        // that would quietly pass even if seeding were broken, which is why it
        // can't be the only list test.
        let empty = Quiz {
            title: "Unit 1".to_string(),
            answers: Vec::new(),
        };
        let mut form = form_for(&empty);
        assert_eq!(form.validate(), Some(empty));
    }

    #[test]
    fn struct_rows_round_trip() {
        // The case that proves the `write_value_into` split: a row is a
        // `FieldSet`, so this nests `begin_list_item` → `begin_field` per struct
        // field. Confusing the two halves fails exactly here.
        let mut form = form_for(&venues());
        assert_eq!(form.validate(), Some(venues()));
    }

    #[test]
    fn rows_are_named_by_index() {
        let form = form_for(&quiz());
        assert_eq!(
            form.leaves(),
            vec![
                ("title".to_string(), "Unit 1".to_string()),
                ("answers.0".to_string(), "alpha".to_string()),
                ("answers.1".to_string(), "beta".to_string()),
            ]
        );
    }

    #[test]
    fn struct_rows_qualify_through_their_index() {
        let form = form_for(&venues());
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            paths,
            vec![
                "places.0.street",
                "places.0.city",
                "places.0.zip",
                "places.1.street",
                "places.1.city",
                "places.1.zip",
            ]
        );
    }

    #[test]
    fn nested_lists_nest_their_indices() {
        let grid = Grid {
            rows: vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string()],
            ],
        };
        let form = form_for(&grid);
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, vec!["rows.0.0", "rows.0.1", "rows.1.0"]);

        let mut form = form;
        assert_eq!(form.validate(), Some(grid));
    }

    #[test]
    fn enum_rows_are_pinned_by_the_value() {
        // Seeding pins each row's variant independently — row 0 and row 1 are
        // different variants of the same enum, and neither needed a choice from
        // the caller because the value itself answered.
        let drawings = Drawings {
            shapes: vec![
                Shape::Circle { radius: 1.5 },
                Shape::Rectangle {
                    width: 2.0,
                    height: 3.0,
                },
            ],
        };
        let form = form_for(&drawings);
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            paths,
            vec!["shapes.0.radius", "shapes.1.width", "shapes.1.height"]
        );

        let mut form = form;
        assert_eq!(form.validate(), Some(drawings));
    }

    #[test]
    fn editing_one_row_leaves_the_others_alone() {
        let mut form = form_for(&venues());
        form.apply(&HashMap::from([(
            "places.1.city".to_string(),
            "Ogdenville".to_string(),
        )]));

        let mut expected = venues();
        expected.places[1].city = "Ogdenville".to_string();
        assert_eq!(form.validate(), Some(expected));
    }

    #[test]
    fn leaves_then_apply_is_an_identity_round_trip() {
        // The widget loop, for lists. Note this reloads into a form of the SAME
        // shape rather than `empty_form` (the way the scalar version of this
        // test does): create mode yields zero rows today, so an `empty_form`
        // here would drop every row on the floor. Swap it once step 4 lands —
        // that substitution is a good check that lengths really are plumbed.
        let form = form_for(&venues());
        let collected: HashMap<String, String> = form.leaves().into_iter().collect();

        let mut reloaded = form_for(&venues());
        reloaded.apply(&collected);
        assert_eq!(reloaded.validate(), Some(venues()));
    }

    #[test]
    fn create_mode_yields_no_rows_yet() {
        // Characterization, not an endorsement: `list_member` has no length to
        // work from without a value, so it builds an empty `ListSet` and
        // `validate` produces `vec![]` with no complaint. This test exists to
        // make that silence visible, and SHOULD start failing at step 4.
        let mut form = empty_form::<Quiz>().expect("no enum fields, so nothing to choose");
        form.apply(&HashMap::from([("title".to_string(), "Unit 1".to_string())]));
        assert_eq!(
            form.validate(),
            Some(Quiz {
                title: "Unit 1".to_string(),
                answers: Vec::new(),
            })
        );
    }
}

/// `Option` composes with every member kind — it is not a fifth kind of its own.
///
/// Scalars and enums behind an `Option` already work; **structs and lists do
/// not**, in either direction. `Some(v)` panics (`begin_field` lands on the
/// `Option` slot, so the inner `begin_field`/`init_list` hits `Option`'s own
/// enum shape), and `None` fails *silently* — `validate()` just returns `None`,
/// because the inner fields are required and `Empty`, so an absent optional
/// container is currently unrepresentable.
///
/// These are the RED target for `option_member`/`OptionalMember`: peel ONE
/// `Option` layer in `member_for_shape` and recurse (the recursion IS the
/// dispatch), wrapping the result in a decorator that owns the
/// `begin_some`/`set_default` frame AND intercepts `validate` — absent means
/// don't validate the inner, which is what unwinds `FormField::required`'s
/// double duty.
///
/// They are `#[ignore]d` so the suite stays green while that lands; run them
/// with `cargo test -- --ignored` and delete each attribute as it passes.
/// `an_absent_optional_struct_still_offers_its_leaves` is deliberately NOT
/// ignored: it passes today and must keep passing.
///
/// Optional *enums* stay covered by `enum_tests` (`edit_mode_round_trips_an_
/// optional_enum`, `create_mode_absent_builds_a_none`, …). Those are the
/// regression guard for retiring `VariantSet::optional`, which `OptionalMember`
/// is meant to subsume.
#[cfg(test)]
mod optional_container_tests {
    use super::tests::Location;
    use super::*;

    /// `Option<Struct>` — the case that panics one way and lies the other.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Contact {
        name: String,
        address: Option<Location>,
    }

    /// `Option<Vec<Scalar>>`. Moved here from `vec_tests`, where it was the
    /// lone ignored list test — it isn't a list bug, it's this one.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Tagged {
        tags: Option<Vec<String>>,
    }

    /// `Option<Vec<Struct>>` — the write path has to compose `begin_some` →
    /// `init_list` → `begin_list_item` → `begin_field`, one frame per layer.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Roster {
        members: Option<Vec<Location>>,
    }

    /// `Option<Vec<Option<Scalar>>>` — two `Option`s at different depths, which
    /// is exactly what "peel one layer per recursion" buys and what the current
    /// single up-front unwrap can never reach.
    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Matrix {
        cells: Option<Vec<Option<String>>>,
    }

    fn springfield() -> Location {
        Location {
            street: "123 Main St".to_string(),
            city: "Springfield".to_string(),
            zip: "12345".to_string(),
        }
    }

    fn contact(address: Option<Location>) -> Contact {
        Contact {
            name: "Ada".to_string(),
            address,
        }
    }

    fn applied(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(p, v)| (p.to_string(), v.to_string()))
            .collect()
    }

    fn paths<T: Clone + Debug + PartialEq + Facet<'static>>(form: &Form<T>) -> Vec<String> {
        form.leaves().into_iter().map(|(p, _)| p).collect()
    }

    // ── Shape of the form ──

    #[test]
    fn an_absent_optional_struct_still_offers_its_leaves() {
        // Passes today, and the decorator must not break it: an absent optional
        // struct still renders its inner inputs, empty. If it hid them there
        // would be no way to *fill one in* — presence is derived from what the
        // user types, so the inputs have to be there to type into.
        //
        // Also pins that `Option` contributes no path segment of its own:
        // `address.street`, never `address.some.street`.
        let form = form_for(&contact(None));
        assert_eq!(
            paths(&form),
            vec!["name", "address.street", "address.city", "address.zip"],
        );
        assert_eq!(
            form.leaves(),
            vec![
                ("name".to_string(), "Ada".to_string()),
                ("address.street".to_string(), String::new()),
                ("address.city".to_string(), String::new()),
                ("address.zip".to_string(), String::new()),
            ],
        );
    }

    // ── Option<Struct> ──

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember — panics today, `must select variant before selecting enum fields`"]
    fn a_present_optional_struct_round_trips() {
        let value = contact(Some(springfield()));
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember — the silent failure, returns None today"]
    fn an_absent_optional_struct_round_trips() {
        // The dangerous one. No panic today: the inner fields are required and
        // `Empty`, so `validate()` reports errors and hands back `None` — an
        // absent optional struct simply cannot be expressed. The wrapper has to
        // intercept `validate`, not just the write path.
        let value = contact(None);
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn create_mode_leaves_an_untouched_optional_struct_absent() {
        // Same rule arriving through the DOM path rather than through seeding —
        // the two boundaries have to agree, as they now do for `""`.
        let mut form = empty_form::<Contact>().expect("no enum fields, so nothing to choose");
        form.apply(&applied(&[("name", "Ada")]));
        assert_eq!(form.validate(), Some(contact(None)));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn filling_in_an_absent_optional_struct_makes_it_present() {
        // `present` is DERIVED, never asked: the user typing into the inner
        // inputs is what makes the container `Some`. No third construction
        // question, and no "is there an address?" checkbox.
        let mut form = form_for(&contact(None));
        form.apply(&applied(&[
            ("address.street", "123 Main St"),
            ("address.city", "Springfield"),
            ("address.zip", "12345"),
        ]));
        assert_eq!(form.validate(), Some(contact(Some(springfield()))));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn blanking_a_present_optional_struct_makes_it_absent() {
        // The inverse, and the same rule one level up from `""` IS absence:
        // every leaf underneath empty ⟺ the container is absent.
        let mut form = form_for(&contact(Some(springfield())));
        form.apply(&applied(&[
            ("address.street", ""),
            ("address.city", ""),
            ("address.zip", ""),
        ]));
        assert_eq!(form.validate(), Some(contact(None)));
    }

    #[test]
    fn a_partly_filled_optional_struct_is_an_error() {
        // Green today, but only vacuously — everything under an `Option<Struct>`
        // is required right now, so *any* partial fill errors. Its real job is
        // as a live guard while the decorator lands: absent has to mean EVERY
        // leaf empty, so a wrapper that treats "some leaf empty" as absent (and
        // quietly drops the street the user typed) fails here.
        //
        // Falls straight out of the two rules above: some leaf is non-empty, so
        // the container is present, so its inner *required* fields are required
        // again. "Optional" is about the whole address, not about each line of
        // it — a street with no city is a half-answered address, not an absent
        // one.
        let mut form = form_for(&contact(None));
        form.apply(&applied(&[("address.street", "123 Main St")]));
        assert_eq!(form.validate(), None);
        assert!(form.has_errors());
    }

    // ── Option<Vec<T>> ──

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember — `init_list` on the Option slot today"]
    fn a_present_optional_list_round_trips() {
        let value = Tagged {
            tags: Some(vec!["x".to_string()]),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn an_absent_optional_list_round_trips() {
        let value = Tagged { tags: None };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember — and CONFIRM the semantics, see comment"]
    fn an_empty_optional_list_collapses_to_absent() {
        // DESIGN QUESTION, not a settled rule. `Some(vec![])` has no leaves at
        // all, so "absent ⟺ every leaf underneath is empty" is vacuously true
        // and it comes back as `None` — the exact analogue of `Some("")`
        // collapsing to `None`, and unrepresentable in the DOM for the same
        // reason (a zero-row list submits nothing).
        //
        // It may not survive create-mode lengths (VEC_PLAN step 4), where a
        // caller could legitimately ask for a present, zero-row list. If that
        // wins, flip this test to expect `Some(vec![])` and derive presence from
        // the *length choice* rather than from emptiness.
        let mut form = form_for(&Tagged {
            tags: Some(Vec::new()),
        });
        assert_eq!(form.validate(), Some(Tagged { tags: None }));
    }

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn an_optional_list_of_structs_round_trips() {
        // Three frames deep on the write path — `begin_some` → `init_list` →
        // `begin_list_item` → `begin_field` — which is where a decorator that
        // forwards `write_into` instead of `write_value_into` comes apart.
        let value = Roster {
            members: Some(vec![springfield()]),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    // ── Composition ──

    #[test]
    #[ignore = "RED: needs option_member/OptionalMember"]
    fn option_peels_one_layer_at_a_time() {
        // `Option<Vec<Option<String>>>` →
        // `OptionalMember(ListSet(rows of OptionalMember(FormField)))`, each
        // layer contributing exactly one frame. Row 1 is empty, so it's `None`
        // by the same rule that makes the whole list present: some leaf under
        // `cells` is non-empty.
        let value = Matrix {
            cells: Some(vec![Some("a".to_string()), None]),
        };
        let form = form_for(&value);
        assert_eq!(paths(&form), vec!["cells.0", "cells.1"]);

        let mut form = form;
        assert_eq!(form.validate(), Some(value));
    }
}

/// `""` is absence — the rule that makes `leaves() -> apply()` an identity.
///
/// The DOM cannot express `Some("")`: HTML5 constraint validation treats an
/// empty input as `valueMissing`, so a `required` field rejects it and an
/// optional one submits nothing distinguishable from "untouched". `apply_leaves`
/// has always honored that. These pin the *other* boundary — seeding — to the
/// same rule, so a value behaves identically whichever path it arrives through.
#[cfg(test)]
mod empty_string_tests {
    use super::*;

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Optional {
        body: Option<String>,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Required {
        body: String,
    }

    /// Push a form's own leaves back through the widget boundary and revalidate —
    /// the path a real submit takes, as opposed to validating the seeded form.
    fn through_the_dom<T>(form: &Form<T>, mut reloaded: Form<T>) -> Option<T>
    where
        T: Clone + Debug + PartialEq + Facet<'static>,
    {
        let collected: HashMap<String, String> = form.leaves().into_iter().collect();
        reloaded.apply(&collected);
        reloaded.validate()
    }

    #[test]
    fn seeding_some_empty_collapses_to_none() {
        // Regression: seeding used to keep `Valid("")` here, so this returned
        // `Some("")` while the DOM path returned `None` for the same model.
        let mut form = form_for(&Optional {
            body: Some(String::new()),
        });
        assert_eq!(form.validate(), Some(Optional { body: None }));
    }

    #[test]
    fn some_empty_agrees_on_both_paths() {
        let value = Optional {
            body: Some(String::new()),
        };
        let form = form_for(&value);
        let seeded = form_for(&value).validate();
        let dom = through_the_dom(&form, empty_form::<Optional>().expect("no enums"));

        assert_eq!(seeded, dom);
        assert_eq!(seeded, Some(Optional { body: None }));
    }

    #[test]
    fn a_required_empty_string_fails_on_both_paths() {
        // The genuine cost of the rule, made explicit: a model holding `""` in a
        // required field can't round-trip. That's HTML5's constraint, not ours —
        // and it now fails the same way whichever path it takes, instead of
        // passing when seeded and erroring through the DOM.
        let value = Required {
            body: String::new(),
        };
        let form = form_for(&value);
        let seeded = form_for(&value).validate();
        let dom = through_the_dom(&form, empty_form::<Required>().expect("no enums"));

        assert_eq!(seeded, None);
        assert_eq!(dom, None);
    }

    #[test]
    fn none_and_some_empty_are_indistinguishable() {
        // Both directions of the same coin: seeding `None` and seeding `Some("")`
        // produce the same form, so nothing downstream can tell them apart.
        let from_none = form_for(&Optional { body: None });
        let from_empty = form_for(&Optional {
            body: Some(String::new()),
        });
        assert_eq!(from_none.leaves(), from_empty.leaves());
    }

    #[test]
    fn non_empty_strings_are_untouched() {
        // Guard on the collapse: it must catch `""` and nothing else. A string of
        // spaces is a real value — trimming is a validator's job, not seeding's.
        let value = Optional {
            body: Some("   ".to_string()),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));

        let value = Required {
            body: "hello".to_string(),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    fn zero_valued_scalars_are_not_empty() {
        // The collapse keys on the *display* string, so it must not swallow
        // falsy-looking numbers and bools — `0`/`0.0`/`false` all render
        // non-empty and stay `Valid`.
        #[derive(Facet, Clone, Debug, PartialEq)]
        struct Falsy {
            count: i32,
            ratio: f64,
            flag: bool,
        }

        let value = Falsy {
            count: 0,
            ratio: 0.0,
            flag: false,
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }
}

