//! The SHAPE walk: turn a `&'static Shape` (plus an optional value to seed
//! from) into a tree of `FormMember`s. One `*_member` helper per kind of thing
//! a shape can be.

use facet::{EnumType, Field, Peek, PeekEnum, ScalarType, Shape, StructType, Type, UserType, Variant};
use std::collections::HashMap;
use crate::choices::VariantChoice;
use crate::fields::{FormField, seed};
use crate::members::{FieldSet, FormMember, ListSet, VariantSet, qualify};

/// One member per declared field of `shape`. `peek` is the value being seeded
/// from, if any — it must describe the same shape.
pub(crate) fn members_for(
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
pub(crate) fn member_for_shape(
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
