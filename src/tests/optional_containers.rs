//! RED targets for `option_member`/`OptionalMember`.

use crate::*;
use facet::Facet;
use std::{collections::HashMap, fmt::Debug};
use super::models::Location;

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
