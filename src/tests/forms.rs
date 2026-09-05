//! End-to-end round trips through `Form<T>`: populate, collect, apply, validate.

use crate::*;
use facet::Facet;
use std::{collections::HashMap, marker::PhantomData};
use super::models::{Event, EventForCreate, Location};

fn text_field(name: &str, value: FieldValue<String>) -> Box<dyn FormMember> {
    Box::new(FormField {
        name: name.to_string(),
        label: None,
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
        text_field("street", street),
        text_field("city", city),
        text_field("zip",  zip),
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
    // Nothing was populated, so every input is blank.
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
    // The actual widget loop: populate a form from a model, hand the raw
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
            text_field("title", FieldValue::Empty),
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
