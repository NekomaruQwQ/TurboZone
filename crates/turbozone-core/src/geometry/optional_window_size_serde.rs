use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};

use super::WindowSize;

/// Serializes an optional euclid size as an optional two-element array.
pub fn serialize<S>(
    size: &Option<WindowSize>,
    serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer {
    size.map(|value| [value.width, value.height]).serialize(serializer)
}

/// Deserializes an optional two-element array as an optional euclid size.
pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<WindowSize>, D::Error>
where
    D: Deserializer<'de> {
    Option::<[i32; 2]>::deserialize(deserializer)
        .map(|size| size.map(|[width, height]| WindowSize::new(width, height)))
}
