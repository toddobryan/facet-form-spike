//! An enum-typed member, locked to one variant chosen before the form existed.

use facet::{Partial, ReflectError};
use std::collections::HashMap;
use crate::choices::VariantChoice;
use crate::error::FormError;
use crate::members::{ABSENT_DISPLAY, FormMember, qualify};

/// An enum-typed field, locked to one answer chosen before the form (per the
/// design — variant choice is a construction parameter, not an editable field).
/// For a [`VariantChoice::Named`] answer it's a `FieldSet` over that variant's
/// fields, plus the name so `write_into` can replay the choice; for
/// [`VariantChoice::Absent`] it holds no members at all and writes `None`.
/// The choice itself is NOT a leaf — it never appears in a path or a submitted
/// value.
#[derive(Clone, Debug)]
pub struct VariantSet {
    pub name: String,
    pub label: Option<String>,
    pub choice: VariantChoice,
    /// Whether this enum sits behind an `Option`, which decides both whether
    /// `Absent` was legal and whether `write_into` needs a `begin_some()` frame.
    pub optional: bool,
    pub members: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,
}

impl FormMember for VariantSet {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn render(&self) -> String {
        match &self.choice {
            // Visible but inert, so the user can see they chose to leave a value
            // out rather than the field silently vanishing. `disabled` also means
            // the browser won't submit it, so `ABSENT_DISPLAY` never round-trips.
            // This is the one member that renders without being a leaf — and the
            // natural spot for a `<select>` if variant choice ever goes live.
            VariantChoice::Absent => {
                let input = format!(
                    r#"<input type="text" name="{}" value="{ABSENT_DISPLAY}" disabled>"#,
                    self.name
                );
                match &self.label {
                    Some(label) => format!("<label>{label} {input}</label>"),
                    None => input,
                }
            }
            VariantChoice::Named(_) => self
                .members
                .iter()
                .map(|m| m.render())
                .collect::<Vec<String>>()
                .join("\n"),
        }
    }

    fn validate(&mut self) {
        self.errors.clear();
        for m in self.members.iter_mut() {
            m.validate();
        }
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }

    fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.members.iter().any(|m| m.has_errors())
    }

    fn raw_value(&self) -> String {
        // Not a scalar — the choice is fixed, not an input of its own.
        String::new()
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        let nested = qualify(prefix, &self.name);
        for m in self.members.iter() {
            m.collect_leaves(&nested, out);
        }
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        let nested = qualify(prefix, &self.name);
        for m in self.members.iter_mut() {
            m.apply_leaves(&nested, values);
        }
    }

    fn write_value_into<'p>(&self, mut partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        match &self.choice {
            // `Option`'s `Default` is `None` whatever the inner type is, so this
            // writes the absent value without ever naming that type — which is
            // the point, since we only have a runtime `Shape` for it.
            VariantChoice::Absent => partial = partial.set_default()?,
            VariantChoice::Named(variant) => {
                // Behind an `Option`, `begin_field` lands on the `Option` slot,
                // not the enum inside it — so descend one level first, or
                // `select_variant_named` looks for the variant among `None`/
                // `Some` and fails. `begin_some` pushes a frame, hence the
                // extra `end()` below.
                if self.optional {
                    partial = partial.begin_some()?;
                }
                // The one thing a plain field set doesn't do: lock in the
                // variant before writing its fields, so `Partial::build`
                // materializes the right one.
                partial = partial.select_variant_named(variant)?;
                for m in self.members.iter() {
                    partial = m.write_into(partial)?;
                }
                if self.optional {
                    partial = partial.end()?; // pops begin_some's frame
                }
            }
        }
        Ok(partial)
    }
}
