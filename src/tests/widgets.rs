//! The widget boundary — signal-minted and fully uncontrolled.

// `dioxus::prelude` exports its own `Location`, so ours needs an explicit
// name to win the glob-import ambiguity.
use super::models::{EventForCreate, Location as ModelLocation};
use crate::*;
use dioxus::prelude::*;
use facet::Facet;
use std::{collections::HashMap, fmt::Debug};

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
