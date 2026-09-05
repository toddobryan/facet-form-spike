//! The two error payloads: one per field, one per form.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormError(pub String);
