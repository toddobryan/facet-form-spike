use facet::{
    EnumType, Facet, Field, Partial, Peek, ReflectError, ScalarType, Shape, StructType, Type,
    UserType,
};
use std::{collections::HashMap, fmt::Debug, marker::PhantomData};

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Event {
    pub id: u32, // server-assigned — not collected by any form field
    pub title: String,
    pub location: Location,
}

/// What a `Form<T>` actually validates into: every field here is genuinely
/// collected by some `FormMember`, so `Partial::build()` never hits an
/// uninitialized field. Surreal assigns `id` on create; on edit, the caller
/// re-attaches the `id` it already had from the original fetch — `Form`
/// itself never needs to know about it.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct EventForCreate {
    pub title: String,
    pub location: Location,
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Location {
    pub street: String,
    pub city: String,
    pub zip: String,
}

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

pub fn form_for<T: Clone + Debug + PartialEq + Facet<'static>>(value: &T) -> Form<T> {
    form_for_impl(Some(value), &HashMap::new())
}

pub fn empty_form<T: Clone + Debug + PartialEq + Facet<'static>>() -> Form<T> {
    form_for_impl(None, &HashMap::new())
}

pub fn empty_form_with_variants<T: Clone + Debug + PartialEq + Facet<'static>>(variants: &HashMap<String, Vec<String>>) -> Form<T> {
    form_for_impl(None, variants)
}

fn form_for_impl<T: Clone + Debug + PartialEq + Facet<'static>>(value: Option<&T>, variants: &HashMap<String, Vec<String>>) -> Form<T> {
    assert!(value.is_some() == variants.is_empty(), "should be impossible to have Some with non-empty variants");
     Form {
        title: None,
        members: members_for(T::SHAPE, value.map(Peek::new), variants, ""),
        errors: Vec::new(),
        _type: PhantomData,
    }
}

/// One member per declared field of `shape`. `peek` is the value being seeded
/// from, if any — it must describe the same shape.
fn members_for(
    shape: &'static Shape,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, Vec<String>>,
    prefix: &str,
) -> Vec<Box<dyn FormMember>> {
    match &shape.ty {
        Type::User(UserType::Struct(struct_type)) => fields_from_struct(struct_type, peek, variants, prefix),
        Type::User(UserType::Enum(enum_type)) => {
            fields_from_enum(enum_type, peek, variants, prefix)
        }
        _ => panic!("form_for only handles structs and enums, got {shape}"),
    }
}

fn fields_from_struct(
    struct_type: &StructType,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, Vec<String>>,
    prefix: &str,
) -> Vec<Box<dyn FormMember>> {
    let peek_struct = peek.map(|p| {
        p.into_struct()
            .expect("shape said struct, so the value peeks as one")
    });

    struct_type
        .fields
        .iter()
        .map(|field| {
            let field_peek = peek_struct.map(|ps| {
                ps.field_by_name(field.name)
                    .expect("field came from this shape, so it exists on the value")
            });
            member_for(field, field_peek, variants, prefix)
        })
        .collect()
}

fn fields_from_enum(
    enum_type: &EnumType,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, Vec<String>>,
    prefix: &str,
) -> Vec<Box<dyn FormMember>> {
    let peek_enum = peek.map(|p| {
        p.into_enum()
            .expect("shape said enum, so the value peeks as one");
    });

    todo!()
}

fn member_for(
    field: &'static Field,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, Vec<String>>,
    prefix: &str,
) -> Box<dyn FormMember> {
    // `Option<X>` means optional, and `X` — not `Option<X>` — is what the
    // widget and the seeded value are actually about.
    let (required, inner_shape) = match field.shape().def.into_option() {
        Ok(option_def) => (false, option_def.t),
        Err(_) => (true, field.shape()),
    };

    // Unwrap the same level on the value side, so `inner_peek` always lines up
    // with `inner_shape`. A `None` here means "nothing to seed from" whether
    // that's create mode or a genuinely absent optional value — both land on
    // `FieldValue::Empty`, which is what we want.
    let inner_peek = match (peek, required) {
        (None, _) => None,
        (Some(p), true) => Some(p),
        (Some(p), false) => p
            .into_option()
            .expect("shape said Option, so the value peeks as one")
            .value(),
    };

    if let Some(scalar) = inner_shape.scalar_type() {
        return scalar_member(scalar, field.name, required, inner_peek).unwrap_or_else(|| {
            panic!(
                "field {} has scalar type {scalar:?}, which isn't in the built-in widget set",
                field.name
            )
        });
    }

    match &inner_shape.ty {
        Type::User(UserType::Struct(_)) => Box::new(FieldSet {
            name: field.name.to_string(),
            label: None,
            members: members_for(inner_shape, inner_peek, variants, prefix),
            errors: Vec::new(),
        }),
        other => panic!("field {} has unsupported type {other:?}", field.name),
    }
}
/// A map from each enum field's qualified path to that enum's variant names.
fn required_variants<T: Facet<'static>>() -> HashMap<String, Vec<String>> {
    // Out-param, not a threaded return: recursion just mutates `out` in place,
    // so there's no borrow to pass back and forth.
    fn walk(out: &mut HashMap<String, Vec<String>>, s: &'static Shape, prefix: &str) {
        match &s.ty {
            Type::User(UserType::Struct(st)) => {
                for f in st.fields.iter() {
                    walk(out, f.shape(), &qualify(prefix, f.name));
                }
            }
            Type::User(UserType::Enum(et)) => {
                out.insert(
                    prefix.to_string(),
                    et.variants.iter().map(|v| v.name.to_string()).collect(),
                );
            }
            // Scalars, Option, List, … — nothing to choose, just skip. (Whether to
            // descend through Option/into variants is one of the open questions.)
            _ => {}
        }
    }

    let mut out = HashMap::new();
    walk(&mut out, T::SHAPE, "");
    out
}

/// The closed set of scalar types with a built-in widget. Anything else needs
/// a custom widget and returns `None` here.
fn scalar_member(
    scalar: ScalarType,
    name: &str,
    required: bool,
    peek: Option<Peek<'_, 'static>>,
) -> Option<Box<dyn FormMember>> {
    macro_rules! dispatch {
        ($( $variant:ident => $ty:ty ),* $(,)?) => {
            match scalar {
                $(
                    ScalarType::$variant => Some(Box::new(FormField::<$ty> {
                        name: name.to_string(),
                        label: None,
                        required,
                        value: seed::<$ty>(peek),
                        errors: Vec::new(),
                    }) as Box<dyn FormMember>),
                )*
                _ => None,
            }
        };
    }

    dispatch! {
        String => String,
        Bool => bool,
        I8 => i8, I16 => i16, I32 => i32, I64 => i64, ISize => isize,
        U8 => u8, U16 => u16, U32 => u32, U64 => u64, USize => usize,
        F32 => f32, F64 => f64,
    }
}

/// Parse a raw input string into `X` using `X`'s own facet parse vtable —
/// the runtime equivalent of the `T: FromStr` bound the macro-based version
/// leaned on.
fn parse_scalar<X>(raw: &str) -> Result<X, FieldError>
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

fn seed<X>(peek: Option<Peek<'_, 'static>>) -> FieldValue<X>
where
    X: Clone + Debug + PartialEq + for<'f> Facet<'f> + 'static,
{
    match peek {
        None => FieldValue::Empty,
        Some(p) => FieldValue::Valid(
            p.get::<X>()
                .expect("scalar_type matched, so this get is the right type")
                .clone(),
        ),
    }
}

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
    fn write_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError>;
}

#[derive(Clone, Debug)]
pub struct FieldSet {
    pub name: String,
    pub label: Option<String>,
    pub members: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,
}

impl FormMember for FieldSet {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn render(&self) -> String {
        // TODO: need to decide whether to wrap in <fieldset> and whether we want to prefix
        // the names somehow, in case there are multiple fieldsets in the same form
        self.members
            .iter()
            .map(|m| m.render())
            .collect::<Vec<String>>()
            .join("\n")
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
        // A field set isn't a scalar — it has no single input of its own.
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

    fn write_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let mut partial = partial.begin_field(&self.name)?;
        for m in self.members.iter() {
            partial = m.write_into(partial)?;
        }
        partial.end()
    }
}

/// `("", "title") -> "title"`, `("location", "street") -> "location.street"`.
fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
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

    fn write_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let mut partial = partial.begin_field(&self.name)?;
        partial = match (&self.value, self.required) {
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
        partial.end()
    }
}

impl Clone for Box<dyn FormMember> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue<T: Clone + Debug + PartialEq> {
    Empty,
    Valid(T),
    Invalid { raw: String, error: FieldError },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormError(pub String);

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Location as ModelLocation;

    fn text_field(name: &str, required: bool, value: FieldValue<String>) -> Box<dyn FormMember> {
        Box::new(FormField {
            name: name.to_string(),
            label: None,
            required,
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
            text_field("street", true, street),
            text_field("city", true, city),
            text_field("zip", true, zip),
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
        let form = empty_form::<Trip>();
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
        let form = empty_form::<EventForCreate>();

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
        // Nothing was seeded, so every input is blank.
        assert!(!rendered.contains(r#"value="Board Game Night""#));
    }

    #[test]
    fn form_for_none_is_invalid_until_filled() {
        let mut form = empty_form::<EventForCreate>();
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
        let mut form = empty_form::<Rsvp>();

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
        let mut form = empty_form::<EventForCreate>();
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
                location: ModelLocation {
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
        let mut form = empty_form::<Rsvp>();
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
        let mut form = empty_form::<Rsvp>();
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
        // The actual widget loop: seed a form from a model, hand the raw
        // strings to the widget layer, take them straight back, and validate.
        // Nothing edited in between, so this must land on the same model.
        let rsvp = Rsvp {
            name: "Ada".to_string(),
            guests: 2,
            note: Some("bringing dessert".to_string()),
        };

        let form = form_for(&rsvp);
        let round_tripped: HashMap<String, String> = form.leaves().into_iter().collect();

        let mut reloaded = empty_form::<Rsvp>();
        reloaded.apply(&round_tripped);

        assert_eq!(reloaded.validate(), Some(rsvp));
    }

    #[test]
    fn empty_event_form_is_invalid() {
        let mut form: Form<Event> = Form {
            title: Some("New Event".to_string()),
            members: vec![
                text_field("title", true, FieldValue::Empty),
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
                    true,
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
}

#[cfg(test)]
mod widget_tests {
    use super::*;
    // `dioxus::prelude` exports its own `Location`, so ours needs an explicit
    // name to win the glob-import ambiguity.
    use crate::Location as ModelLocation;
    use dioxus::prelude::*;
    use std::collections::HashMap;

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
        let form = use_hook(|| empty_form::<EventForCreate>());
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
                    let mut form = empty_form::<EventForCreate>();
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

        let mut form = empty_form::<EventForCreate>();
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
}
