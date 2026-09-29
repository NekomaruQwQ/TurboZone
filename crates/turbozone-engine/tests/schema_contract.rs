use euclid::default::Size2D;
use turbozone_core::*;
use turbozone_engine::config_parser::*;

use schemars::Schema;
use schemars::generate::SchemaSettings;
use serde_json::json;

/// Derives the editor-facing schema with the same input-oriented settings as generation.
fn config_schema() -> Schema {
    SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<Config>()
}

/// Checks every size field's derived array schema against serialization and the real loader.
#[test]
fn size_schema_matches_array_serialization_and_runtime_bounds() {
    let schema = config_schema();
    let definitions = &schema.as_value()["$defs"];
    let selector_default = definitions["ResizeRule"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["type"] == "array")
        .unwrap();
    let exact = definitions["ResizeRule"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|variant| variant.pointer("/properties/exact"))
        .unwrap();
    let serialized = serde_json::to_value(ResizeRule::Exact {
        exact: Size2D::new(1, MAX_SIZE_DIMENSION),
    })
    .unwrap();
    let serialized_size = &serialized["exact"];

    for (field, size_schema) in [
        ("resize", selector_default),
        ("resize.exact", exact),
        (
            "resize.default",
            &definitions["ResizeSelector"]["properties"]["default"],
        ),
        (
            "resize.min",
            &definitions["ResizeSelector"]["properties"]["min"],
        ),
        (
            "resize.max",
            &definitions["ResizeSelector"]["properties"]["max"],
        ),
        (
            "window.min",
            &definitions["WindowFilter"]["properties"]["min"],
        ),
        (
            "window.max",
            &definitions["WindowFilter"]["properties"]["max"],
        ),
    ] {
        let minimum = size_schema["items"]["minimum"].as_i64().unwrap();
        let maximum = size_schema["items"]["maximum"].as_i64().unwrap();
        assert_eq!(size_schema["items"]["type"], "integer");
        assert_eq!(serialized_size, &json!([minimum, maximum]));
        assert_eq!(
            size_schema["minItems"],
            serialized_size.as_array().unwrap().len()
        );
        assert_eq!(size_schema["maxItems"], size_schema["minItems"]);

        // Exercise each axis at and just outside the limits, using the real loader.
        for (value, valid) in [
            (minimum - 1, false),
            (minimum, true),
            (maximum, true),
            (maximum + 1, false),
        ] {
            for size in [[value, 1], [1, value]] {
                let source = format!("[[rules]]\nname = 'size'\n{field} = {size:?}");
                let accepted = parse_config(&source).is_some();
                assert_eq!(
                    accepted, valid,
                    "{field} = {size:?} must match the schema's limits"
                );
            }
        }
    }
}
