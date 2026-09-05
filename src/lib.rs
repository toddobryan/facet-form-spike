//! A runtime-reflection alternative to formoxus's derive macros: build a form
//! from a model's `Facet` shape instead of from a hand-written form struct.

pub mod build;
pub mod choices;
pub mod error;
pub mod fields;
pub mod form;
pub mod members;

// A flat root, so `use facet_form_spike::*` (and the test modules' `use crate::*`)
// reaches the whole vocabulary without knowing which module each name lives in.
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

#[cfg(test)]
mod tests;
