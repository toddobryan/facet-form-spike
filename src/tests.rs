//! The test suite, one module per concern. These live inside the crate rather
//! than in `tests/` because several reach crate-private items.

pub mod models;

mod empty_strings;
mod enums;
mod forms;
mod optional_containers;
mod vecs;
mod widgets;
mod roundtrip;
