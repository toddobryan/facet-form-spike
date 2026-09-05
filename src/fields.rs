//! Leaf members: a single input, its parsed value, and the two vtable-driven
//! conversions that replace `FromStr`/`Display` bounds on the model.

use facet::{Facet, Partial, Peek, ReflectError};
use std::{collections::HashMap, fmt::Debug};
use crate::error::FieldError;
use crate::members::{FormMember, qualify};

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue<T: Clone + Debug + PartialEq> {
    Empty,
    Valid(T),
    Invalid { raw: String, error: FieldError },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormField<T: Clone + Debug + PartialEq + for<'f> Facet<'f>> {
    pub name: String,
    pub label: Option<String>,
    pub required: bool,
    pub value: FieldValue<T>,
    pub errors: Vec<FieldError>,
}

impl<T: Clone + Debug + PartialEq + for<'f> Facet<'f> + 'static> FormMember for FormField<T> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn raw_value(&self) -> String {
        match &self.value {
            FieldValue::Empty => String::new(),
            // Formatted through facet's display vtable rather than a `Display`
            // bound on `T` — the exact mirror of `parse_scalar` going the other
            // way. `{t:?}` would be wrong here: `Debug` quotes strings, and
            // `parse_scalar` faithfully parses those quotes back into the value.
            FieldValue::Valid(t) => Peek::new(t).to_string(),
            FieldValue::Invalid { raw, .. } => raw.clone(),
        }
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        out.push((qualify(prefix, &self.name), self.raw_value()));
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        let Some(raw) = values.get(&qualify(prefix, &self.name)) else {
            return; // nothing supplied for this field; leave it as it stands
        };

        // An empty input means "unfilled", which is what `Empty` encodes —
        // that's what lets required-validation still fire on a blanked field.
        if raw.is_empty() {
            self.value = FieldValue::Empty;
            return;
        }

        // No `FromStr` bound on `T`: facet's own parse vtable does this from
        // the shape, so a custom type only has to derive `Facet`, not
        // implement `FromStr` the way the macro-based version required.
        self.value = match parse_scalar::<T>(raw) {
            Ok(t) => FieldValue::Valid(t),
            Err(error) => FieldValue::Invalid {
                raw: raw.clone(),
                error,
            },
        };
    }

    fn render(&self) -> String {
        let value = self.raw_value();
        let input = format!(
            r#"<input type="text" name="{}" value="{}">"#,
            self.name, value
        );
        match &self.label {
            Some(label) => format!("<label>{label} {input}</label>"),
            None => input,
        }
    }

    fn validate(&mut self) {
        self.errors.clear();
        if self.required && matches!(self.value, FieldValue::Empty) {
            self.errors
                .push(FieldError("This field is required.".to_string()));
        }
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }

    fn has_errors(&self) -> bool {
        !self.errors.is_empty() || matches!(self.value, FieldValue::Invalid { .. })
    }

    fn write_value_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let partial = match (&self.value, self.required) {
            (FieldValue::Valid(t), true) => partial.set(t.clone())?,
            // Required-vs-optional was decided from the Model's own shape at
            // construction time (`Def::Option` — see the earlier discussion):
            // `required == false` means the Model's field is really
            // `Option<T>`, so the value written back has to be wrapped/`None`
            // to match, not the bare `T` the `required` branch writes.
            (FieldValue::Valid(t), false) => partial.set(Some(t.clone()))?,
            (FieldValue::Empty, false) => partial.set(None::<T>)?,
            (FieldValue::Empty, true) | (FieldValue::Invalid { .. }, _) => {
                unreachable!("write_into should only run after validate() has confirmed no errors")
            }
        };
        Ok(partial)
    }
}

/// Parse a raw input string into `X` using `X`'s own facet parse vtable —
/// the runtime equivalent of the `T: FromStr` bound the macro-based version
/// leaned on.
pub(crate) fn parse_scalar<X>(raw: &str) -> Result<X, FieldError>
where
    X: Clone + Debug + PartialEq + for<'f> Facet<'f> + 'static,
{
    let partial = Partial::alloc::<X>().map_err(|e| FieldError(e.to_string()))?;
    let partial = partial
        .parse_from_str(raw)
        .map_err(|_| FieldError(format!("{raw:?} isn't a valid {}", X::SHAPE)))?;
    partial
        .build()
        .map_err(|e| FieldError(e.to_string()))?
        .materialize::<X>()
        .map_err(|e| FieldError(e.to_string()))
}

pub(crate) fn seed<X>(peek: Option<Peek<'_, 'static>>) -> FieldValue<X>
where
    X: Clone + Debug + PartialEq + for<'f> Facet<'f> + 'static,
{
    match peek {
        None => FieldValue::Empty,
        // `""` IS absence, and that has to hold at BOTH boundaries. `apply_leaves`
        // already collapses an empty input to `Empty`; without the same collapse
        // here, seeding kept `Some("")` alive and `leaves() -> apply()` silently
        // stopped being an identity — the very invariant the uncontrolled design
        // rests on. Comparing the *display* string is what makes the two agree
        // exactly, since that's the string `raw_value` would have emitted.
        //
        // Only `String` can actually reach this: `true`/`0`/`0.0` are never empty.
        // The cost is that a required `String` holding `""` can't round-trip — but
        // that's HTML5's rule, not ours (an empty required input is `valueMissing`),
        // so no browser form could round-trip it either. Failing the same way on
        // both paths beats depending on which path the value arrived through.
        Some(p) if p.to_string().is_empty() => FieldValue::Empty,
        Some(p) => FieldValue::Valid(
            p.get::<X>()
                .expect("scalar_type matched, so this get is the right type")
                .clone(),
        ),
    }
}
