//! `T -> String -> T` must be the identity for every scalar a `FormField` can
//! hold, because that is the trip a value takes through the DOM: `leaves()`
//! formats it through facet's display vtable, the browser hands the string back,
//! and `apply_leaves` parses it through the parse vtable.
//!
//! **Where this does and does not bite today.** `FieldValue::Valid(T)` holds the
//! *typed* value and `write_value_into` does `partial.set(t.clone())`, so
//! `form_for(&m).validate() == Some(m)` holds by construction — that path never
//! passes through a string at all. Only the DOM path depends on this. If
//! `FieldValue` ever becomes raw-canonical (`Valid(String)`), the model path
//! starts depending on it too, and these tests become the only thing standing
//! between a lossy `Display` and silent data corruption.
//!
//! For floats this is a language guarantee rather than luck: Rust's `Display`
//! for `f32`/`f64` emits the shortest decimal string that parses back to the
//! identical value.
//!
//! **Custom scalars cannot be checked here.** A user-supplied type reaching a
//! `FormField` must uphold the same property in its own `Facet` display/parse
//! vtables; nothing in the type system enforces it. [`round_trips`] is public to
//! the test suite so such a type can be held to the same standard.

use crate::fields::parse_scalar;
use facet::{Facet, Peek};
use std::fmt::Debug;

/// Assert `T -> String -> T` for each value, reporting the intermediate string
/// on failure — without it, a mismatch tells you nothing about which direction
/// broke.
fn round_trips<T>(values: &[T])
where
    T: Clone + Debug + PartialEq + for<'f> Facet<'f> + 'static,
{
    for v in values {
        let s = Peek::new(v).to_string();
        match parse_scalar::<T>(&s) {
            Ok(back) => assert_eq!(back, *v, "{v:?} formatted as {s:?}, parsed back as {back:?}"),
            Err(e) => panic!("{v:?} formatted as {s:?}, which failed to parse: {e:?}"),
        }
    }
}

#[test]
fn strings_round_trip() {
    // The empty string is here for completeness, but note it can never actually
    // reach `Valid`: "" IS absence, so it collapses to `Empty` at both
    // boundaries (see `empty_strings`).
    round_trips(&[
        String::new(),
        "   ".to_string(),
        "a\"b".to_string(),
        "héllo ☃".to_string(),
        "line\nbreak".to_string(),
        "trailing space ".to_string(),
    ]);
}

#[test]
fn bools_round_trip() {
    round_trips(&[true, false]);
}

#[test]
fn signed_integers_round_trip_at_their_limits() {
    // MIN is the interesting one: its magnitude has no positive counterpart, so
    // a formatter that produced `-` plus `abs()` would overflow.
    round_trips(&[i8::MIN, -1, 0, i8::MAX]);
    round_trips(&[i16::MIN, 0, i16::MAX]);
    round_trips(&[i32::MIN, 0, i32::MAX]);
    round_trips(&[i64::MIN, 0, i64::MAX]);
    round_trips(&[isize::MIN, 0, isize::MAX]);
}

#[test]
fn unsigned_integers_round_trip_at_their_limits() {
    round_trips(&[0u8, u8::MAX]);
    round_trips(&[0u16, u16::MAX]);
    round_trips(&[0u32, u32::MAX]);
    round_trips(&[0u64, u64::MAX]);
    round_trips(&[0usize, usize::MAX]);
}

#[test]
fn floats_round_trip_including_the_awkward_ones() {
    // `1.0/3.0` and `0.1 + 0.2` are the classic shortest-repr traps, and the
    // subnormal/limit values are where a naive `{:.N}` formatter loses bits.
    round_trips(&[
        0.0f32, -0.0, 1.0 / 3.0, f32::MIN, f32::MAX,
        f32::MIN_POSITIVE, f32::EPSILON, f32::INFINITY, f32::NEG_INFINITY,
    ]);
    round_trips(&[
        0.0f64, -0.0, 1.0 / 3.0, 0.1 + 0.2, f64::MIN, f64::MAX,
        f64::MIN_POSITIVE, f64::EPSILON, f64::INFINITY, f64::NEG_INFINITY,
    ]);
}

#[test]
fn nan_survives_the_trip_even_though_it_is_never_equal_to_itself() {
    // NaN can't go through `round_trips` — `NaN == NaN` is false by IEEE rule,
    // not because anything was lost. Check the property that actually matters:
    // what comes back is still a NaN.
    let s = Peek::new(&f64::NAN).to_string();
    let back = parse_scalar::<f64>(&s).expect("NaN should parse back");
    assert!(back.is_nan(), "f64::NAN formatted as {s:?}, parsed back as {back:?}");
}
