//! Common serialization helpers used by the data model.

pub(crate) use serde::*;
pub(crate) use euclid::default::Size2D;

/// Constructs the inferred type's default value.
pub fn default<T: Default>() -> T {
    T::default()
}

/// Returns whether a value can be omitted when serializing defaulted fields.
pub fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}
