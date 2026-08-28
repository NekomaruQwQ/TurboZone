use turbozone_core::*;

use serde_json::json;

/// Keeps editor completion aligned with Serde renames such as `relocate` to `move`.
#[test]
fn schema_exposes_serialized_names_instead_of_rust_field_names() {
    let rule = Rule {
        name: "app".to_owned(),
        relocate: true,
        program: ProgramFilter {
            name: Some(Pattern::Exact("app.exe".to_owned())),
            ..Default::default()
        },
        ..Default::default()
    };
    let serialized = serde_json::to_value(rule).unwrap();
    let schema = Config::schema();
    let properties = schema.pointer("/definitions/Rule/properties").unwrap();
    for name in serialized.as_object().unwrap().keys() {
        assert!(
            properties.get(name).is_some(),
            "serialized field {name} needs a schema"
        );
    }
    assert!(properties.get("move").is_some() && properties.get("relocate").is_none());
}

/// Guards against making sizes required when annotating their array elements.
#[test]
fn optional_size_arrays_remain_nullable_and_not_required() {
    let schema = Config::schema();
    for (definition, fields) in [
        ("ResizeSelector", ["default", "min", "max"].as_slice()),
        ("WindowFilter", ["min", "max"].as_slice()),
    ] {
        let object = &schema.as_value()["definitions"][definition];
        for field in fields {
            let types = object["properties"][field]["type"].as_array().unwrap();
            assert!(types.contains(&json!("array")) && types.contains(&json!("null")));
            assert!(
                !object["required"]
                    .as_array()
                    .is_some_and(|required| required.contains(&json!(field)))
            );
        }
    }
}

/// Prevents completion and validation from treating distinct resize modes as one object.
#[test]
fn untagged_resize_schema_keeps_exact_and_selector_fields_separate() {
    let schema = Config::schema();
    let variants = schema
        .pointer("/definitions/ResizeRule/anyOf")
        .unwrap()
        .as_array()
        .unwrap();
    let serialized = serde_json::to_value(ResizeRule::Exact { exact: [640, 480] }).unwrap();
    let exact = variants
        .iter()
        .find(|variant| variant["properties"].get("exact").is_some())
        .unwrap();
    let exact_fields: Vec<_> = exact["properties"].as_object().unwrap().keys().collect();
    assert_eq!(
        exact_fields,
        serialized.as_object().unwrap().keys().collect::<Vec<_>>()
    );
    assert_eq!(exact["required"], json!(["exact"]));
    assert_eq!(exact["additionalProperties"], false);

    let selector = schema.pointer("/definitions/ResizeSelector").unwrap();
    assert!(selector["properties"].get("exact").is_none());
    assert_eq!(selector["additionalProperties"], false);
    assert!(variants.iter().any(|variant| variant["type"] == "boolean"));
    assert!(variants.iter().any(|variant| variant["type"] == "array"));
}

/// Checks every size field's derived array schema against serialization and the real loader.
#[test]
fn size_schema_matches_array_serialization_and_runtime_bounds() {
    let schema = Config::schema();
    let definitions = &schema.as_value()["definitions"];
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
        exact: [1, i32::MAX],
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
                let accepted =
                    parse_config(&source).is_ok_and(|report| report.diagnostics.is_empty());
                assert_eq!(
                    accepted, valid,
                    "{field} = {size:?} must match the schema's limits"
                );
            }
        }
    }
}
