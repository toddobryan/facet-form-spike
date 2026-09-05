//! `Form<T>` and the three public constructors — one per mode, so the illegal
//! combination (a value AND variant choices) cannot be written.

use facet::{Facet, Partial, Peek};
use std::{collections::HashMap, fmt::Debug, marker::PhantomData};
use crate::build::members_for;
use crate::choices::{MissingVariants, VariantChoice, missing_variants};
use crate::error::FormError;
use crate::members::FormMember;

#[derive(Clone, Debug)]
pub struct Form<T: Clone + Debug + Facet<'static>> {
    pub title: Option<String>,
    pub members: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,

    pub _type: PhantomData<T>,
}

impl<T: Clone + Debug + PartialEq + Facet<'static>> Form<T> {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.members.iter().any(|m| m.has_errors())
    }

    pub fn validate(&mut self) -> Option<T> {
        self.errors.clear();
        for m in self.members.iter_mut() {
            m.validate();
        }
        if self.has_errors() {
            None
        } else {
            let mut partial =
                Partial::alloc::<T>().expect("alloc should never fail for a concrete T");
            for m in self.members.iter() {
                partial = m
                    .write_into(partial)
                    .expect("write_into should succeed once validate() found no errors");
            }
            Some(
                partial
                    .build()
                    .expect("build should succeed once every field was written")
                    .materialize::<T>()
                    .expect("materialized shape should match T — write_into wrote the wrong thing if not"),
            )
        }
    }

    /// Every leaf input in the form, as `(qualified_path, raw_value)` — the
    /// list the widget layer turns into one signal apiece.
    pub fn leaves(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for m in self.members.iter() {
            m.collect_leaves("", &mut out);
        }
        out
    }

    /// Take raw widget values back in, keyed by the same qualified paths
    /// [`leaves`](Self::leaves) hands out. Call this on submit, before
    /// `validate()`.
    pub fn apply(&mut self, values: &HashMap<String, String>) {
        for m in self.members.iter_mut() {
            m.apply_leaves("", values);
        }
    }

    /// Take values straight off a submitted `<form>`. Dioxus's
    /// `FormData::values()` hands back `(name, FormValue)` pairs keyed by each
    /// input's `name` attribute — which is exactly the qualified path
    /// [`leaves`](Self::leaves) emitted — so no per-field signal is needed to
    /// track edits: the DOM already did it.
    pub fn apply_form_values(&mut self, values: &[(String, String)]) {
        let map: HashMap<String, String> = values.iter().cloned().collect();
        self.apply(&map);
    }

    pub fn render(&self) -> String {
        let title = self
            .title
            .as_ref()
            .map(|t| format!("<h2>{}</h2>\n", t))
            .unwrap_or("".to_string());
        let members_rendered = self
            .members
            .iter()
            .map(|m| m.render())
            .collect::<Vec<String>>()
            .join("\n");
        format!("{}{}", title, members_rendered)
    }
}

/// Edit mode. Infallible: the value itself pins every variant, so there is
/// nothing left for a caller to choose.
pub fn form_for<T: Clone + Debug + PartialEq + Facet<'static>>(value: &T) -> Form<T> {
    form_for_impl(Some(value), &HashMap::new())
}

/// Create mode with no choices supplied — fails with [`MissingVariants`] if `T`
/// contains any enum at all.
pub fn empty_form<T: Clone + Debug + PartialEq + Facet<'static>>()
-> Result<Form<T>, MissingVariants> {
    empty_form_with_variants(&HashMap::new())
}

/// Create mode. Returns the still-needed choices rather than panicking, so a
/// caller can render the next round of pickers and try again — the loop
/// [`missing_variants`] describes.
pub fn empty_form_with_variants<T: Clone + Debug + PartialEq + Facet<'static>>(
    variants: &HashMap<String, VariantChoice>,
) -> Result<Form<T>, MissingVariants> {
    let missing = missing_variants::<T>(variants);
    if !missing.is_empty() {
        return Err(MissingVariants(missing));
    }
    // Every enum reachable under `T` now has a valid choice, so the construction
    // below can't hit an unchosen one.
    Ok(form_for_impl(None, variants))
}

fn form_for_impl<T: Clone + Debug + PartialEq + Facet<'static>>(
    value: Option<&T>,
    variants: &HashMap<String, VariantChoice>,
) -> Form<T> {
    assert!(
        value.is_none() || variants.is_empty(),
        "should be impossible to have Some with non-empty variants"
    );
     Form {
        title: None,
        members: members_for(T::SHAPE, value.map(Peek::new), variants, ""),
        errors: Vec::new(),
        _type: PhantomData,
    }
}
