//! Model fixtures shared by more than one test module. Types used by a single
//! module stay in that module.

use facet::Facet;

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

/// Used by both `enums` and `vecs` — the latter as `Vec<Shape>`, to prove each
/// row's variant is pinned independently. `#[repr(u8)]` is required for facet
/// to derive on an enum (it needs the discriminant repr).
#[derive(Facet, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}
