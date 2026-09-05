//! Config geometry must be usable directly while preserving the authored pair format.
//! Check every size position independently so optional fields and untagged resize
//! variants cannot silently acquire a different Serde representation.

use euclid::default::Size2D;
use serde_json::json;
use turbozone_core::{Config, ResizeRule, parse_config};

#[test]
fn every_size_field_decodes_as_geometry_and_serializes_as_a_pair() {
    for field in [
        "resize", "resize.exact", "resize.default", "resize.min", "resize.max",
        "window.min", "window.max",
    ] {
        let source = format!("[[rules]]\nname = 'app'\n{field} = [1440, 900]");
        let config: Config = toml::from_str(&source).unwrap();
        let rule = &config.rules[0];
        let actual = match field {
            "resize" => match rule.resize {
                ResizeRule::SelectorDefault(size) => Some(size),
                _ => panic!("shorthand must retain its authored variant"),
            },
            "resize.exact" => match rule.resize {
                ResizeRule::Exact { exact } => Some(exact),
                _ => panic!("exact must retain its authored variant"),
            },
            "resize.default" => rule.resize.selector().unwrap().default,
            "resize.min" => rule.resize.selector().unwrap().min,
            "resize.max" => rule.resize.selector().unwrap().max,
            "window.min" => rule.window.min,
            "window.max" => rule.window.max,
            _ => panic!("unknown size field in test fixture: {field}"),
        };
        assert_eq!(actual, Some(Size2D::new(1440, 900)), "{field}");

        let serialized = serde_json::to_value(&config).unwrap();
        let pointer = format!("/rules/0/{}", field.replace('.', "/"));
        assert_eq!(serialized.pointer(&pointer), Some(&json!([1440, 900])), "{field}");

        let restored = parse_config(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), serialized, "{field}");
    }
}
