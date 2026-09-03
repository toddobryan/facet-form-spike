use facet::{
    EnumType, Facet, Field, Partial, Peek, PeekEnum, ReflectError, ScalarType, Shape, StructType,
    Type, UserType, Variant,
};
use std::{collections::HashMap, fmt::Debug, marker::PhantomData};

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

/// A caller's answer for one enum path.
///
/// Deliberately not a bare `String`: `enum Filter { None, ByDate { .. } }` is a
/// perfectly ordinary model, so a `"None"` sentinel would make "leave this
/// optional field empty" indistinguishable from "pick the `None` variant."
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariantChoice {
    /// An `Option<Enum>` left empty. Only legal where the enum sits behind an
    /// `Option` — [`VariantOptions::optional`] says where that is.
    Absent,
    /// This variant, by name.
    Named(String),
}

/// What a caller may answer for one enum path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantOptions {
    /// True when the enum sits behind an `Option`, making
    /// [`VariantChoice::Absent`] a legal answer — a picker should offer a
    /// "none" entry in addition to the variants.
    pub optional: bool,
    /// Variant names, in declaration order.
    pub variants: Vec<String>,
}

/// The enum choices a form still needs before it can be built.
///
/// The payload is exactly what a variant-picker UI needs — which path, what the
/// options are, and whether "none" is among them — so handling this error *is*
/// the create-a-record flow, not defensive boilerplate bolted onto it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingVariants(pub HashMap<String, VariantOptions>);

impl std::fmt::Display for MissingVariants {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut paths: Vec<&String> = self.0.keys().collect();
        paths.sort();
        write!(f, "no variant chosen for: ")?;
        for (i, path) in paths.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            let opts = &self.0[*path];
            write!(f, "{path} (one of {:?}", opts.variants)?;
            if opts.optional {
                write!(f, " or none")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl std::error::Error for MissingVariants {}

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

/// One member per declared field of `shape`. `peek` is the value being seeded
/// from, if any — it must describe the same shape.
fn members_for(
    shape: &'static Shape,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
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
    variants: &HashMap<String, VariantChoice>,
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
            member_for(field, field_peek, variants, &qualify(prefix, field.name))
        })
        .collect()
}

/// The variant to build for an enum shape. Edit mode reads it off the value
/// (the value pins it); create mode takes it from the caller's chosen-variant
/// map, keyed by this enum's qualified path. Shared by the top-level
/// `fields_from_enum` and `member_for`'s enum-field arm (which also needs the
/// variant's *name*), so the selection logic lives in exactly one place.
/// `None` means [`VariantChoice::Absent`] — an `Option<Enum>` with no value.
///
/// `seeding` is whether we're in edit mode, and it's what disambiguates a
/// `peek_enum` of `None`: while seeding, that means the value's `Option` really
/// was `None` (so: absent, no choice needed — this is why edit mode is
/// infallible even for optional enums). While not seeding, it just means there
/// is no value at all and the answer comes from the caller's map.
fn chosen_variant(
    enum_type: &EnumType,
    peek_enum: Option<PeekEnum<'_, 'static>>,
    seeding: bool,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Option<&'static Variant> {
    let variant_name: String = if seeding {
        match peek_enum {
            Some(pe) => pe
                .variant_name_active()
                .expect("an enum value always has an active variant")
                .to_string(),
            // Seeding from a value whose optional enum is `None`.
            None => return None,
        }
    } else {
        // Unreachable from the public constructors: `empty_form_with_variants`
        // pre-checks with `missing_variants`, which reports an unanswered path,
        // one naming a variant this enum doesn't have, and `Absent` where it
        // isn't legal.
        match variants.get(prefix) {
            Some(VariantChoice::Named(name)) => name.clone(),
            Some(VariantChoice::Absent) => return None,
            None => panic!(
                "no variant chosen for the enum at {prefix:?} — missing_variants should have caught this"
            ),
        }
    };

    Some(
        enum_type
            .variants
            .iter()
            .find(|v| v.name == variant_name)
            .unwrap_or_else(|| {
                panic!("{variant_name:?} is not a variant of this enum — missing_variants should have caught this")
            }),
    )
}

/// One member per field of the chosen `variant`, seeded from that variant's
/// fields on the value in edit mode. Paths accumulate under this enum's path
/// just like a struct's fields — the variant name is NOT part of the path
/// (it's locked; `write_into` replays it via `select_variant_named`).
fn variant_members(
    variant: &'static Variant,
    peek_enum: Option<PeekEnum<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Vec<Box<dyn FormMember>> {
    variant
        .data
        .fields
        .iter()
        .map(|field| {
            let field_peek = peek_enum.and_then(|pe| {
                pe.field_by_name(field.name)
                    .expect("field belongs to the active variant, so access can't error")
            });
            member_for(field, field_peek, variants, &qualify(prefix, field.name))
        })
        .collect()
}

/// Members for a top-level enum model (the whole `Form<T>` is an enum). Enum
/// *fields* don't come through here — `member_for` builds a `VariantSet` for
/// those; this is only the `members_for` dispatch for a bare-enum `T`.
fn fields_from_enum(
    enum_type: &EnumType,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Vec<Box<dyn FormMember>> {
    let peek_enum = peek.map(|p| {
        p.into_enum()
            .expect("shape said enum, so the value peeks as one")
    });
    let variant = chosen_variant(enum_type, peek_enum, peek.is_some(), variants, prefix)
        .expect("a top-level enum model is not behind an Option, so it can't be Absent");
    variant_members(variant, peek_enum, variants, prefix)
}

/// The member for one declared *field* — the common case, where the name and
/// shape both come from the field itself.
fn member_for(
    field: &'static Field,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Box<dyn FormMember> {
    member_for_shape(field.shape(), field.name, peek, variants, prefix)
}

/// The member for a shape that is *named separately* from where it came from.
///
/// Split out of [`member_for`] because a list element has no [`Field`] and so no
/// `field.name` — its name is its index (`"0"`, `"1"`, …), supplied by the
/// enclosing `ListSet`. Everything below is shape-driven and never looked at the
/// `Field` for anything but those two things, so the split is pure motion.
fn member_for_shape(
    shape: &'static Shape,
    name: &str,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Box<dyn FormMember> {
    // `Option<X>` means optional, and `X` — not `Option<X>` — is what the
    // widget and the seeded value are actually about.
    let (required, inner_shape) = match shape.def.into_option() {
        Ok(option_def) => (false, option_def.t),
        Err(_) => (true, shape),
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
        return scalar_member(scalar, name, required, inner_peek).unwrap_or_else(|| {
            panic!(
                "field {name} has scalar type {scalar:?}, which isn't in the built-in widget set"
            )
        });
    } else if let Ok(list_def) = inner_shape.def.into_list() {
        // SEAM: `list_member(_list_def.t, name, inner_peek, variants, prefix)`,
        // building one row per element via `member_for_shape` with the index as
        // the row's name. See VEC_PLAN.md.
        return list_member(list_def.t, name, inner_peek, variants, prefix);
    }

    match &inner_shape.ty {
        Type::User(UserType::Struct(_)) => {
            struct_member(inner_shape, name, inner_peek, variants, prefix)
        }
        Type::User(UserType::Enum(enum_type)) => enum_member(
            enum_type,
            name,
            required,
            inner_peek,
            peek.is_some(),
            variants,
            prefix,
        ),
        other => panic!("field {name} has unsupported type {other:?}"),
    }
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

/// A list-typed field: a [`ListSet`] whose rows are named by their index, so
/// the leaf paths (`answer_choices.0.text`) fall out of the same `qualify`
/// nesting a struct's fields use — no special casing anywhere downstream.
///
/// `shape` is the *element* shape (`ListDef::t`), not the `Vec`'s.
///
/// Edit mode takes the row count from the value. Create mode has no value to
/// count, and the length is a construction parameter that isn't plumbed through
/// yet — see VEC_PLAN.md step 4 — so it yields zero rows for now.
fn list_member(
    shape: &'static Shape,
    name: &str,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Box<dyn FormMember> {
    let rows = peek
        .map(|p| {
            let list = p
                .into_list()
                .expect("shape said list, so the value peeks as one");
            list.iter()
                .enumerate()
                .map(|(i, element)| {
                    // The index IS the row's name, and — unlike `struct_member`,
                    // which passes `prefix` straight through — nothing upstream
                    // has qualified it on yet, so do it here. Keeping this in
                    // step with `collect_leaves` is what keeps the `variants`
                    // map keys and the leaf paths the same strings.
                    let row = i.to_string();
                    member_for_shape(shape, &row, Some(element), variants, &qualify(prefix, &row))
                })
                .collect()
        })
        .unwrap_or_default();

    Box::new(ListSet {
        name: name.to_string(),
        label: None,
        rows,
        errors: Vec::new(),
    })
}


/// A struct-typed field: a [`FieldSet`] over that struct's own fields, whose
/// paths accumulate under this field's name.
fn struct_member(
    shape: &'static Shape,
    name: &str,
    peek: Option<Peek<'_, 'static>>,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Box<dyn FormMember> {
    Box::new(FieldSet {
        name: name.to_string(),
        label: None,
        members: members_for(shape, peek, variants, prefix),
        errors: Vec::new(),
    })
}

/// An enum-typed field: a [`VariantSet`] locked to one already-decided variant.
///
/// `seeding` is the *outer* `peek.is_some()` — whether a value was supplied at
/// all — which is what lets `chosen_variant` tell "this optional enum's value
/// really was `None`" apart from "no value; consult the chosen-variant map."
/// It is deliberately not `peek.is_some()` on the unwrapped `peek` below, since
/// that would conflate the two.
fn enum_member(
    enum_type: &EnumType,
    name: &str,
    required: bool,
    peek: Option<Peek<'_, 'static>>,
    seeding: bool,
    variants: &HashMap<String, VariantChoice>,
    prefix: &str,
) -> Box<dyn FormMember> {
    let peek_enum = peek.map(|p| {
        p.into_enum()
            .expect("shape said enum, so the value peeks as one")
    });
    let variant = chosen_variant(enum_type, peek_enum, seeding, variants, prefix);
    Box::new(VariantSet {
        name: name.to_string(),
        label: None,
        // `None` from `chosen_variant` is exactly `Absent` — either the caller
        // chose it, or we're seeding from a value whose `Option` was `None`.
        choice: match variant {
            Some(v) => VariantChoice::Named(v.name.to_string()),
            None => VariantChoice::Absent,
        },
        optional: !required,
        // No variant means no fields to build.
        members: variant
            .map(|v| variant_members(v, peek_enum, variants, prefix))
            .unwrap_or_default(),
        errors: Vec::new(),
    })
}
/// A map from each enum field's qualified path to that enum's variant names,
/// before any choices have been made. The starting point of the iterative
/// disclosure loop — see [`missing_variants`].
fn required_variants<T: Facet<'static>>() -> HashMap<String, VariantOptions> {
    missing_variants::<T>(&HashMap::new())
}

/// The enum choices still needed to build a form for `T`, given the choices
/// already made — keyed by qualified path, valued by that enum's variant names.
///
/// **Inherently iterative.** A variant has to be chosen before its fields are
/// visible at all, so choosing one can reveal *more* enums underneath it that
/// were unreachable a moment ago. Callers loop: ask, present those pickers,
/// record the answers, ask again, until this comes back empty. That's why the
/// naive one-shot walk was wrong — it could only ever see the enums reachable
/// without making a single choice.
fn missing_variants<T: Facet<'static>>(
    chosen: &HashMap<String, VariantChoice>,
) -> HashMap<String, VariantOptions> {
    // Out-param, not a threaded return: recursion just mutates `out` in place,
    // so there's no borrow to pass back and forth. `optional` tracks whether we
    // just came through an `Option`, which decides whether `Absent` is a legal
    // answer for the enum we're about to hit.
    fn walk(
        out: &mut HashMap<String, VariantOptions>,
        s: &'static Shape,
        prefix: &str,
        chosen: &HashMap<String, VariantChoice>,
        optional: bool,
    ) {
        // `Option<X>` doesn't change the path — `member_for` unwraps it without
        // qualifying — so look straight through it for enums inside, carrying
        // the optionality down one level.
        if let Ok(option_def) = s.def.into_option() {
            walk(out, option_def.t, prefix, chosen, true);
            return;
        }

        match &s.ty {
            Type::User(UserType::Struct(st)) => {
                // Each field's own shape decides its optionality, so it resets.
                for f in st.fields.iter() {
                    walk(out, f.shape(), &qualify(prefix, f.name), chosen, false);
                }
            }
            Type::User(UserType::Enum(et)) => {
                let record = |out: &mut HashMap<String, VariantOptions>| {
                    out.insert(
                        prefix.to_string(),
                        VariantOptions {
                            optional,
                            variants: et.variants.iter().map(|v| v.name.to_string()).collect(),
                        },
                    );
                };

                match chosen.get(prefix) {
                    // Left empty, and legal here: nothing inside to reach, so
                    // this path is fully answered.
                    Some(VariantChoice::Absent) if optional => {}
                    // `Absent` on an enum that isn't behind an `Option` is not a
                    // real answer — report it as still-needed so the caller sees
                    // what the actual options are.
                    Some(VariantChoice::Absent) => record(out),
                    Some(VariantChoice::Named(name)) => {
                        match et.variants.iter().find(|v| v.name == *name) {
                            // Chosen: descend into that variant's own fields,
                            // which is where newly-revealed enums show up.
                            Some(variant) => {
                                for f in variant.data.fields.iter() {
                                    walk(out, f.shape(), &qualify(prefix, f.name), chosen, false);
                                }
                            }
                            // Named something this enum doesn't have — same
                            // treatment as unanswered, which hands back the real
                            // options rather than failing obscurely later.
                            None => record(out),
                        }
                    }
                    None => record(out),
                }
            }
            // Scalars, List, … — nothing to choose here.
            _ => {}
        }
    }

    let mut out = HashMap::new();
    walk(&mut out, T::SHAPE, "", chosen, false);
    out
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

    fn write_value_into<'p>(&self, mut partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        for m in self.members.iter() {
            partial = m.write_into(partial)?;
        }
        Ok(partial)
    }
}

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

#[derive(Clone, Debug)]
pub struct ListSet {
    pub name: String,
    pub label: Option<String>,
    pub rows: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,
}

impl FormMember for ListSet {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn render(&self) -> String {
        self.rows
            .iter()
            .map(|r| r.render())
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn raw_value(&self) -> String {
        String::new()
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        let nested = qualify(prefix, &self.name);
        for r in self.rows.iter() {
            r.collect_leaves(&nested, out);
        }
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        let nested = qualify(prefix, &self.name);
        for r in self.rows.iter_mut() {
            r.apply_leaves(&nested, values);
        }
    }

    fn validate(&mut self) {
        self.errors.clear();
        for r in self.rows.iter_mut() {
            r.validate();
        }
    }

    fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.rows.iter().any(|r| r.has_errors())
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }

    fn write_value_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let mut partial = partial.init_list()?;
        for r in self.rows.iter() {
            partial = partial.begin_list_item()?;
            partial = r.write_value_into(partial)?;
            partial = partial.end()?;
        }
        Ok(partial)
    }
}

/// Shown for an `Option<Enum>` the user chose to leave empty — in the disabled
/// input below, and (later) as the "none" entry in a variant picker. Display
/// only: a disabled input isn't submitted, so this never comes back through
/// `FormData::values()` and can't be mistaken for a value. That's what keeps it
/// from reintroducing the sentinel problem `VariantChoice` exists to avoid.
const ABSENT_DISPLAY: &str = "--none--";

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
        let form = empty_form::<Trip>().expect("no enum fields, so nothing to choose");
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
        let form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");

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
        let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
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
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");

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
        let mut form = empty_form::<EventForCreate>().expect("no enum fields, so nothing to choose");
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
                location: Location {
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
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
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
        let mut form = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
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

        let mut reloaded = empty_form::<Rsvp>().expect("no enum fields, so nothing to choose");
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
    use super::tests::{EventForCreate, Location as ModelLocation};
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
}

#[cfg(test)]
mod enum_tests {
    use super::*;
    use super::tests::Location;

    // Vec-free enums, per the agreed first target — isolates the enum work from
    // the still-unproven `Vec`/`Def::List` handling. `#[repr(u8)]` is required
    // for facet to derive on an enum (it needs the discriminant repr).
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Shape {
        Circle { radius: f64 },
        Rectangle { width: f64, height: f64 },
    }

    // Unit variants — to prove they still enumerate even with no fields.
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Mode {
        Fast,
        Slow,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Drawing {
        pub name: String,
        pub shape: Shape,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Config {
        pub shape: Shape,
        pub mode: Mode,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Outer {
        pub title: String,
        pub drawing: Drawing,
    }

    // An enum reachable only *through* another enum's variant — the shape that
    // makes variant discovery iterative rather than one-shot.
    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Inner {
        A { x: f64 },
        B { y: f64 },
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    #[repr(u8)]
    pub enum Outer2 {
        First { inner: Inner },
        Second { n: u32 },
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Doc {
        pub outer: Outer2,
    }

    /// An enum behind an `Option` — the case nothing covered until now.
    #[derive(Facet, Clone, Debug, PartialEq)]
    pub struct Sketch {
        pub name: String,
        pub shape: Option<Shape>,
    }

    /// Expected `missing_variants` output for enums NOT behind an `Option`.
    fn variants(pairs: &[(&str, &[&str])]) -> HashMap<String, VariantOptions> {
        opt_variants(pairs, false)
    }

    /// Same, for enums that ARE behind an `Option` — `Absent` is legal, so a
    /// picker should offer a "none" entry alongside the variants.
    fn optional_variants(pairs: &[(&str, &[&str])]) -> HashMap<String, VariantOptions> {
        opt_variants(pairs, true)
    }

    fn opt_variants(pairs: &[(&str, &[&str])], optional: bool) -> HashMap<String, VariantOptions> {
        pairs
            .iter()
            .map(|(path, vs)| {
                (
                    path.to_string(),
                    VariantOptions {
                        optional,
                        variants: vs.iter().map(|v| v.to_string()).collect(),
                    },
                )
            })
            .collect()
    }

    /// Shorthand for building a choice map.
    fn chose(pairs: &[(&str, VariantChoice)]) -> HashMap<String, VariantChoice> {
        pairs
            .iter()
            .map(|(path, c)| (path.to_string(), c.clone()))
            .collect()
    }

    fn named(name: &str) -> VariantChoice {
        VariantChoice::Named(name.to_string())
    }

    // A struct with no enum fields anywhere has nothing to choose — the empty
    // map is exactly what makes `empty_form::<T>().expect("no enum fields, so nothing to choose")` safe for such a `T`.
    #[test]
    fn no_enum_fields_yields_empty_map() {
        assert_eq!(required_variants::<Location>(), HashMap::new());
    }

    // One enum field: keyed by the field name, listing that enum's variants in
    // declaration order (Circle before Rectangle).
    #[test]
    fn single_enum_field_lists_its_variants_in_declaration_order() {
        assert_eq!(
            required_variants::<Drawing>(),
            variants(&[("shape", &["Circle", "Rectangle"])]),
        );
    }

    // Two enum fields → two entries; `mode`'s unit variants still enumerate.
    #[test]
    fn multiple_enum_fields_including_unit_variants() {
        assert_eq!(
            required_variants::<Config>(),
            variants(&[
                ("shape", &["Circle", "Rectangle"]),
                ("mode", &["Fast", "Slow"]),
            ]),
        );
    }

    // An enum nested under a struct field is keyed by its qualified path —
    // `drawing.shape`, never `shape` — the same field-name qualification
    // `collect_leaves` uses, so two nested enums can't collide.
    #[test]
    fn nested_enum_field_is_qualified_by_path() {
        assert_eq!(
            required_variants::<Outer>(),
            variants(&[("drawing.shape", &["Circle", "Rectangle"])]),
        );
    }

    // ── Discovery: optionality and the legality of Absent ──

    // An enum behind an `Option` is reported as optional, so a picker knows to
    // offer a "none" entry alongside the variants.
    #[test]
    fn optional_enum_is_discovered_as_optional() {
        assert_eq!(
            required_variants::<Sketch>(),
            optional_variants(&[("shape", &["Circle", "Rectangle"])]),
        );
    }

    // Choosing Absent answers an optional enum completely — nothing is left.
    #[test]
    fn absent_satisfies_an_optional_enum() {
        let chosen = chose(&[("shape", VariantChoice::Absent)]);
        assert_eq!(missing_variants::<Sketch>(&chosen), HashMap::new());
    }

    // But Absent is not a real answer for an enum that isn't behind an Option:
    // it stays reported, so the caller sees the options it actually has.
    #[test]
    fn absent_is_rejected_where_the_enum_is_not_optional() {
        let chosen = chose(&[("shape", VariantChoice::Absent)]);
        assert_eq!(
            missing_variants::<Drawing>(&chosen),
            variants(&[("shape", &["Circle", "Rectangle"])]),
        );
    }

    // Naming a variant this enum doesn't have is treated the same as not
    // answering — which hands back the real options instead of failing later
    // inside construction.
    #[test]
    fn an_unknown_variant_name_reports_the_real_options() {
        let chosen = chose(&[("shape", named("Triangle"))]);
        assert_eq!(
            missing_variants::<Drawing>(&chosen),
            variants(&[("shape", &["Circle", "Rectangle"])]),
        );
    }

    // ── Option<Enum>: currently broken, fixed by the Absent/begin_some work ──

    // KNOWN FAILING (deliberate). `member_for` unwraps the `Option` and builds a
    // `VariantSet` whose `write_into` does `begin_field("shape")` — landing on the
    // `Option<Shape>` slot, not the `Shape` inside it — and then asks it for a
    // variant named "Circle". facet models `Option` as an enum over `None`/`Some`,
    // so that lookup can't succeed. The fix is `begin_some()` before selecting.
    #[test]
    fn edit_mode_round_trips_an_optional_enum() {
        let sketch = Sketch {
            name: "Doodle".to_string(),
            shape: Some(Shape::Circle { radius: 1.5 }),
        };
        let mut form = form_for(&sketch);
        assert_eq!(form.validate(), Some(sketch));
    }

    // The other half: `None` should round-trip to `None`, with no leaves under
    // `shape` at all.
    #[test]
    fn edit_mode_round_trips_an_absent_optional_enum() {
        let sketch = Sketch {
            name: "Doodle".to_string(),
            shape: None,
        };
        let mut form = form_for(&sketch);
        assert_eq!(form.validate(), Some(sketch));
    }

    // Create mode, Absent: no leaves under `shape` at all, and validate()
    // produces `None` for it.
    #[test]
    fn create_mode_absent_builds_a_none() {
        let chosen = chose(&[("shape", VariantChoice::Absent)]);
        let mut form = empty_form_with_variants::<Sketch>(&chosen)
            .expect("Absent is a legal answer for an optional enum");

        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, vec!["name"], "Absent contributes no leaves");

        form.apply_form_values(&[("name".to_string(), "Doodle".to_string())]);
        assert_eq!(
            form.validate(),
            Some(Sketch {
                name: "Doodle".to_string(),
                shape: None,
            }),
        );
    }

    // Create mode, a chosen variant behind an Option: its fields appear under
    // the enum's path, and validate() wraps the result in `Some`.
    #[test]
    fn create_mode_named_behind_an_option_builds_a_some() {
        let chosen = chose(&[("shape", named("Circle"))]);
        let mut form = empty_form_with_variants::<Sketch>(&chosen)
            .expect("Circle is a variant of Shape");

        form.apply_form_values(&[
            ("name".to_string(), "Doodle".to_string()),
            ("shape.radius".to_string(), "2.5".to_string()),
        ]);
        assert_eq!(
            form.validate(),
            Some(Sketch {
                name: "Doodle".to_string(),
                shape: Some(Shape::Circle { radius: 2.5 }),
            }),
        );
    }

    // The absent field is visible but inert, so the user can see they left it
    // out. Disabled means the browser won't submit it, so ABSENT_DISPLAY never
    // comes back as a value.
    #[test]
    fn absent_renders_a_disabled_placeholder() {
        let sketch = Sketch {
            name: "Doodle".to_string(),
            shape: None,
        };
        let html = form_for(&sketch).render();
        assert!(html.contains(ABSENT_DISPLAY), "html: {html}");
        assert!(html.contains("disabled"), "html: {html}");
    }

    // ── Iterative disclosure: choices reveal further choices ──

    // The whole loop in one test. `outer.inner` does not exist as a question
    // until `outer` is answered with the variant that contains it — which is
    // why a single up-front walk could never have found it.
    #[test]
    fn choosing_a_variant_reveals_the_enums_inside_it() {
        // Nothing chosen: only the outer enum is reachable.
        assert_eq!(
            missing_variants::<Doc>(&chose(&[])),
            variants(&[("outer", &["First", "Second"])]),
            "outer.inner must be invisible before outer is answered",
        );

        // Answering `outer` with the variant that holds an enum reveals it.
        assert_eq!(
            missing_variants::<Doc>(&chose(&[("outer", named("First"))])),
            variants(&[("outer.inner", &["A", "B"])]),
        );

        // The other branch holds no enum, so answering it finishes the loop.
        assert_eq!(
            missing_variants::<Doc>(&chose(&[("outer", named("Second"))])),
            HashMap::new(),
        );

        // Answering the revealed question finishes the first branch too.
        assert_eq!(
            missing_variants::<Doc>(&chose(&[
                ("outer", named("First")),
                ("outer.inner", named("A")),
            ])),
            HashMap::new(),
        );
    }

    // The two walks agree: once `missing_variants` is empty, construction
    // succeeds — that's the invariant keeping the `Result` from lying.
    #[test]
    fn a_fully_answered_nested_enum_builds_and_round_trips() {
        let chosen = chose(&[("outer", named("First")), ("outer.inner", named("A"))]);
        assert_eq!(missing_variants::<Doc>(&chosen), HashMap::new());

        let mut form =
            empty_form_with_variants::<Doc>(&chosen).expect("every reachable enum is answered");

        // The doubly-nested leaf is qualified the whole way down.
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, vec!["outer.inner.x"]);

        form.apply_form_values(&[("outer.inner.x".to_string(), "1.25".to_string())]);
        assert_eq!(
            form.validate(),
            Some(Doc {
                outer: Outer2::First {
                    inner: Inner::A { x: 1.25 },
                },
            }),
        );
    }

    // Partial answers stay errors, and the error names what is still open —
    // which is what a caller renders as the next round of pickers.
    #[test]
    fn a_half_answered_nested_enum_reports_only_what_is_still_open() {
        let chosen = chose(&[("outer", named("First"))]);
        let err = empty_form_with_variants::<Doc>(&chosen)
            .expect_err("outer.inner is revealed but unanswered");
        assert_eq!(err.0, variants(&[("outer.inner", &["A", "B"])]));
    }

    // ── Construction: the enum field actually round-trips through validate() ──

    // Create mode: the caller picks the variant, the chosen variant's fields
    // become leaves under the enum's path, and validate() builds that variant.
    #[test]
    fn empty_form_with_variants_builds_and_validates_the_chosen_variant() {
        let chosen = chose(&[("shape", named("Circle"))]);
        let mut form = empty_form_with_variants::<Drawing>(&chosen).expect("every enum has a chosen variant");

        // Only the chosen variant's field is present, keyed under `shape`.
        let paths: Vec<String> = form.leaves().into_iter().map(|(p, _)| p).collect();
        assert!(paths.contains(&"name".to_string()), "paths: {paths:?}");
        assert!(paths.contains(&"shape.radius".to_string()), "paths: {paths:?}");

        form.apply_form_values(&[
            ("name".to_string(), "My Drawing".to_string()),
            ("shape.radius".to_string(), "3.5".to_string()),
        ]);
        assert_eq!(
            form.validate(),
            Some(Drawing {
                name: "My Drawing".to_string(),
                shape: Shape::Circle { radius: 3.5 },
            }),
        );
    }

    // Edit mode: the value pins the variant (no map needed) and seeds its
    // fields; validate() replays the same variant via select_variant_named.
    #[test]
    fn form_for_round_trips_an_enum_field() {
        let drawing = Drawing {
            name: "Rect".to_string(),
            shape: Shape::Rectangle {
                width: 2.0,
                height: 4.0,
            },
        };
        let mut form = form_for(&drawing);
        assert_eq!(form.validate(), Some(drawing));
    }

    // The enum lives one struct deep — exercises the qualified path
    // (`drawing.shape.…`) through both construction and write_into.
    #[test]
    fn nested_enum_field_round_trips() {
        let outer = Outer {
            title: "T".to_string(),
            drawing: Drawing {
                name: "N".to_string(),
                shape: Shape::Circle { radius: 1.0 },
            },
        };
        let mut form = form_for(&outer);
        assert_eq!(form.validate(), Some(outer));
    }

    // OPEN QUESTIONS — deliberately not asserted, because the behavior isn't
    // decided yet. Flagging so we choose on purpose rather than by accident:
    //
    //   1. `Option<SomeEnum>` field — is it in the map (needs a variant IF
    //      present) or omitted (absence is a legal, choice-free state)?
    //   2. A top-level enum `T` itself (`required_variants::<Shape>()`) — what's
    //      the key, `""`? Or is a bare-enum model simply out of scope?
    //   3. An enum nested *inside a variant's* fields — does the walk recurse
    //      through variants, and if so what are those paths (they only exist
    //      once a parent variant is chosen)?
}

/// `Vec` / `Def::List` — edit mode. Rows are members named by their index, so
/// every path convention here falls out of the same `qualify` nesting a struct's
/// fields use, with no list-specific code in `collect_leaves`/`apply_leaves`.
///
/// Everything below seeds from a value. Create mode is deliberately absent: the
/// row count isn't in the shape, so it's a construction parameter like an enum's
/// variant, and it isn't plumbed through yet (VEC_PLAN.md step 4).
#[cfg(test)]
mod vec_tests {
    use super::enum_tests::Shape;
    use super::tests::Location;
    use super::*;

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
        // that would quietly pass even if seeding were broken, which is why it
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
        // Seeding pins each row's variant independently — row 0 and row 1 are
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
}

/// `Option` composes with every member kind — it is not a fifth kind of its own.
///
/// Scalars and enums behind an `Option` already work; **structs and lists do
/// not**, in either direction. `Some(v)` panics (`begin_field` lands on the
/// `Option` slot, so the inner `begin_field`/`init_list` hits `Option`'s own
/// enum shape), and `None` fails *silently* — `validate()` just returns `None`,
/// because the inner fields are required and `Empty`, so an absent optional
/// container is currently unrepresentable.
///
/// These are the RED target for `option_member`/`OptionalMember`: peel ONE
/// `Option` layer in `member_for_shape` and recurse (the recursion IS the
/// dispatch), wrapping the result in a decorator that owns the
/// `begin_some`/`set_default` frame AND intercepts `validate` — absent means
/// don't validate the inner, which is what unwinds `FormField::required`'s
/// double duty.
///
/// They are `#[ignore]d` so the suite stays green while that lands; run them
/// with `cargo test -- --ignored` and delete each attribute as it passes.
/// `an_absent_optional_struct_still_offers_its_leaves` is deliberately NOT
/// ignored: it passes today and must keep passing.
///
/// Optional *enums* stay covered by `enum_tests` (`edit_mode_round_trips_an_
/// optional_enum`, `create_mode_absent_builds_a_none`, …). Those are the
/// regression guard for retiring `VariantSet::optional`, which `OptionalMember`
/// is meant to subsume.
#[cfg(test)]
mod optional_container_tests {
    use super::tests::Location;
    use super::*;

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
}

/// `""` is absence — the rule that makes `leaves() -> apply()` an identity.
///
/// The DOM cannot express `Some("")`: HTML5 constraint validation treats an
/// empty input as `valueMissing`, so a `required` field rejects it and an
/// optional one submits nothing distinguishable from "untouched". `apply_leaves`
/// has always honored that. These pin the *other* boundary — seeding — to the
/// same rule, so a value behaves identically whichever path it arrives through.
#[cfg(test)]
mod empty_string_tests {
    use super::*;

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Optional {
        body: Option<String>,
    }

    #[derive(Facet, Clone, Debug, PartialEq)]
    struct Required {
        body: String,
    }

    /// Push a form's own leaves back through the widget boundary and revalidate —
    /// the path a real submit takes, as opposed to validating the seeded form.
    fn through_the_dom<T>(form: &Form<T>, mut reloaded: Form<T>) -> Option<T>
    where
        T: Clone + Debug + PartialEq + Facet<'static>,
    {
        let collected: HashMap<String, String> = form.leaves().into_iter().collect();
        reloaded.apply(&collected);
        reloaded.validate()
    }

    #[test]
    fn seeding_some_empty_collapses_to_none() {
        // Regression: seeding used to keep `Valid("")` here, so this returned
        // `Some("")` while the DOM path returned `None` for the same model.
        let mut form = form_for(&Optional {
            body: Some(String::new()),
        });
        assert_eq!(form.validate(), Some(Optional { body: None }));
    }

    #[test]
    fn some_empty_agrees_on_both_paths() {
        let value = Optional {
            body: Some(String::new()),
        };
        let form = form_for(&value);
        let seeded = form_for(&value).validate();
        let dom = through_the_dom(&form, empty_form::<Optional>().expect("no enums"));

        assert_eq!(seeded, dom);
        assert_eq!(seeded, Some(Optional { body: None }));
    }

    #[test]
    fn a_required_empty_string_fails_on_both_paths() {
        // The genuine cost of the rule, made explicit: a model holding `""` in a
        // required field can't round-trip. That's HTML5's constraint, not ours —
        // and it now fails the same way whichever path it takes, instead of
        // passing when seeded and erroring through the DOM.
        let value = Required {
            body: String::new(),
        };
        let form = form_for(&value);
        let seeded = form_for(&value).validate();
        let dom = through_the_dom(&form, empty_form::<Required>().expect("no enums"));

        assert_eq!(seeded, None);
        assert_eq!(dom, None);
    }

    #[test]
    fn none_and_some_empty_are_indistinguishable() {
        // Both directions of the same coin: seeding `None` and seeding `Some("")`
        // produce the same form, so nothing downstream can tell them apart.
        let from_none = form_for(&Optional { body: None });
        let from_empty = form_for(&Optional {
            body: Some(String::new()),
        });
        assert_eq!(from_none.leaves(), from_empty.leaves());
    }

    #[test]
    fn non_empty_strings_are_untouched() {
        // Guard on the collapse: it must catch `""` and nothing else. A string of
        // spaces is a real value — trimming is a validator's job, not seeding's.
        let value = Optional {
            body: Some("   ".to_string()),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));

        let value = Required {
            body: "hello".to_string(),
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }

    #[test]
    fn zero_valued_scalars_are_not_empty() {
        // The collapse keys on the *display* string, so it must not swallow
        // falsy-looking numbers and bools — `0`/`0.0`/`false` all render
        // non-empty and stay `Valid`.
        #[derive(Facet, Clone, Debug, PartialEq)]
        struct Falsy {
            count: i32,
            ratio: f64,
            flag: bool,
        }

        let value = Falsy {
            count: 0,
            ratio: 0.0,
            flag: false,
        };
        let mut form = form_for(&value);
        assert_eq!(form.validate(), Some(value));
    }
}
