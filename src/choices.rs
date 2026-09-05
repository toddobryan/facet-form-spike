//! Facts a form needs that its model's shape does not carry — today that means
//! which enum variant to build — plus the pre-flight walk that asks for them.

use facet::{Facet, Shape, Type, UserType};
use std::collections::HashMap;
use crate::members::qualify;

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

/// A map from each enum field's qualified path to that enum's variant names,
/// before any choices have been made. The starting point of the iterative
/// disclosure loop — see [`missing_variants`].
pub fn required_variants<T: Facet<'static>>() -> HashMap<String, VariantOptions> {
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
pub fn missing_variants<T: Facet<'static>>(
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
