//! The `FormMember` trait every part of a form implements, and the container
//! kinds that make a form a tree rather than a flat list.

use facet::{Partial, ReflectError};
use std::{collections::HashMap, fmt::Debug};

mod field_set;
mod list_set;
mod variant_set;

pub use field_set::FieldSet;
pub use list_set::ListSet;
pub use variant_set::VariantSet;

pub trait FormMember: Debug {
    fn name(&self) -> String;
    fn label(&self) -> Option<String>;
    fn render(&self) -> String; // Element, later
    /// This member's current value as the string an `<input>` would show.
    /// Containers have no scalar value of their own and return `""` — the
    /// widget layer only ever asks leaves for this.
    fn raw_value(&self) -> String;
    /// Flatten this member's leaves into `(qualified_path, raw_value)` pairs,
    /// e.g. `("location.street", "123 Main St")`. Paths are qualified because
    /// two field sets in one form can each have a `street`.
    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>);
    /// The reverse of [`collect_leaves`](Self::collect_leaves): each leaf looks
    /// up its own qualified path in `values` and takes the raw string back in.
    /// This is the "shuffle back" from widget state into plain form data.
    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>);
    fn validate(&mut self);
    fn has_errors(&self) -> bool;
    fn clone_box(&self) -> Box<dyn FormMember>;
    fn write_value_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError>;
    
    fn write_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let mut partial = partial.begin_field(&self.name())?;
        partial = self.write_value_into(partial)?;
        partial.end()
    }
}

impl Clone for Box<dyn FormMember> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Shown for an `Option<Enum>` the user chose to leave empty — in the disabled
/// input below, and (later) as the "none" entry in a variant picker. Display
/// only: a disabled input isn't submitted, so this never comes back through
/// `FormData::values()` and can't be mistaken for a value. That's what keeps it
/// from reintroducing the sentinel problem `VariantChoice` exists to avoid.
pub(crate) const ABSENT_DISPLAY: &str = "--none--";

/// `("", "title") -> "title"`, `("location", "street") -> "location.street"`.
pub(crate) fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}
