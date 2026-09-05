//! `Vec`/`Def::List` in edit mode.

use crate::*;
use facet::Facet;
use std::collections::HashMap;
use super::models::{Location, Shape};

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
    // that would quietly pass even if populating were broken, which is why it
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
    // Populating pins each row's variant independently — row 0 and row 1 are
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
