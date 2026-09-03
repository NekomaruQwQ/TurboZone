use schemars::generate::SchemaSettings;
use smol_str::format_smolstr;
use turbozone_core::{Config, Pattern, parse_config};

const DESCRIPTION: &str =
    "A deliberately long Unicode description for \u{5de5}\u{5177} windows";
const STARTS_WITH: &str = "\u{c4}BCDEFGHIJKLMNOPQRSTUVWXYZ";
const ENDS_WITH: &str = "\u{7d42}\u{7aef}";
const CONTAINS: &str = "\u{5de5}\u{5177}";

/// Exercises both inline and heap-backed configuration text through the public
/// serialization contract so representation changes cannot truncate user input.
#[test]
fn configuration_strings_round_trip_short_long_and_unicode_values() {
    let source = format_smolstr!(
        "[[rules]]\nname = 'app'\ndescription = '{DESCRIPTION}'\n\
         program.name.starts_with = '{STARTS_WITH}'\n\
         program.name.ends_with = '{ENDS_WITH}'\n\
         program.name.contains = '{CONTAINS}'");
    let config: Config = toml::from_str(&source).unwrap();
    let serialized = serde_json::to_vec(&config).unwrap();
    let reparsed: Config = serde_json::from_slice(&serialized).unwrap();
    let rule = &reparsed.rules[0];
    let Some(&Pattern::Partial {
        ref starts_with,
        ref ends_with,
        ref contains,
    }) =
        rule.program.name.as_ref() else {
            panic!("program name must remain a partial pattern");
        };

    assert_eq!(rule.name, "app");
    assert_eq!(rule.description, DESCRIPTION);
    assert_eq!(starts_with, STARTS_WITH);
    assert_eq!(ends_with, ENDS_WITH);
    assert_eq!(contains, CONTAINS);
}

/// The storage type is an implementation detail; editor-facing schemas must
/// continue advertising ordinary JSON strings for every configured text field.
#[test]
fn schema_represents_smol_strings_as_json_strings() {
    let schema =
        SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<Config>();
    let value = schema.as_value();
    let rule = &value["$defs"]["Rule"]["properties"];
    let pattern_variants = value["$defs"]["Pattern"]["anyOf"].as_array().unwrap();
    let partial = pattern_variants
        .iter()
        .find(|variant| variant["properties"].is_object())
        .unwrap();

    assert_eq!(rule["name"]["type"], "string");
    assert_eq!(rule["description"]["type"], "string");
    assert!(pattern_variants.iter().any(|variant| variant["type"] == "string"));
    for field in ["starts_with", "ends_with", "contains"] {
        assert_eq!(partial["properties"][field]["type"], "string");
    }
}

/// Normalization rebuilds immutable text, but must retain the compiler's existing
/// Unicode case-folding behavior even when a pattern exceeds inline capacity.
#[test]
fn compiler_normalizes_long_unicode_patterns_without_truncation() {
    let source = format_smolstr!("[[rules]]\nname = 'app'\nprogram.name = '{STARTS_WITH}'");
    let report = parse_config(&source).unwrap();
    let rule = &report.runtime.rules[0];

    assert!(rule.matches(Some("\u{e4}bcdefghijklmnopqrstuvwxyz"), "", "", None));
}

/// Diagnostic payload storage is not observable; its stable text remains the
/// contract consumed by startup logging and external callers.
#[test]
fn diagnostics_preserve_their_public_text() {
    let report = parse_config(
        r#"[[rules]]
name = "app"
program.path = 'C:\Tool.exe'"#).unwrap();
    let message = format_smolstr!("{}", report.diagnostics[0].error);

    assert_eq!(
        message,
        "rules[0].program.path must use forward slashes; backslashes are not accepted");
}
