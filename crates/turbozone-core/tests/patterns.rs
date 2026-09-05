use turbozone_core::{Pattern, parse_config};

#[test]
fn exact_patterns_compare_the_whole_input_including_empty_strings() {
    let pattern = Pattern::Exact("Tool".into());
    assert!(pattern.matches("Tool"));
    assert!(!pattern.matches("tool"));
    assert!(!pattern.matches("Tool Window"));
    assert!(pattern.matches_ignore_case("tOoL"));
    assert!(!pattern.matches_ignore_case("TOOL WINDOW"));

    let empty = Pattern::Exact("".into());
    assert!(empty.matches(""));
    assert!(empty.matches_ignore_case(""));
    assert!(!empty.matches("Tool"));
    assert!(!empty.matches_ignore_case("Tool"));
}

#[test]
fn partial_patterns_require_every_configured_component() {
    let pattern = Pattern::Partial {
        starts_with: "Tool".into(),
        ends_with: "Ready".into(),
        contains: " - ".into(),
    };
    assert!(pattern.matches("Tool - Ready"));
    assert!(pattern.matches_ignore_case("TOOL - READY"));
    for input in ["Other - Ready", "Tool - Busy", "Tool Ready", ""] {
        assert!(!pattern.matches(input), "{input}");
        assert!(!pattern.matches_ignore_case(input), "{input}");
    }
    assert!(!pattern.matches("tool - ready"));
}

#[test]
fn each_partial_component_can_be_used_on_its_own() {
    for pattern in [
        Pattern::Partial { starts_with: "Tool".into(), ends_with: "".into(), contains: "".into() },
        Pattern::Partial { starts_with: "".into(), ends_with: "Ready".into(), contains: "".into() },
        Pattern::Partial { starts_with: "".into(), ends_with: "".into(), contains: " - ".into() },
    ] {
        assert!(pattern.matches("Tool - Ready"));
        assert!(pattern.matches_ignore_case("TOOL - READY"));
        assert!(!pattern.matches("unrelated"));
        assert!(!pattern.matches_ignore_case("unrelated"));
    }
}

#[test]
fn manually_constructed_empty_partials_fail_closed() {
    let pattern = Pattern::Partial {
        starts_with: "".into(), ends_with: "".into(), contains: "".into(),
    };
    for input in ["", "Tool"] {
        assert!(!pattern.matches(input));
        assert!(!pattern.matches_ignore_case(input));
    }
}

/// Short and long literals exercise both SmolStr storage paths. Dotted I expands
/// when lowercased, while sharp S must not acquire full case-folding semantics.
#[test]
fn insensitive_matching_preserves_unicode_lowercase_semantics() {
    for (literal, input) in [
        ("\u{c4}BC", "\u{e4}bc"),
        ("\u{c4}BCDEFGHIJKLMNOPQRSTUVWXYZ", "\u{e4}bcdefghijklmnopqrstuvwxyz"),
        ("\u{130}", "i\u{307}"),
        ("\u{1e9e}", "\u{df}"),
    ] {
        let pattern = Pattern::Exact(literal.into());
        assert!(pattern.matches_ignore_case(input));
        assert!(!pattern.matches(input));
    }
    assert!(!Pattern::Exact("\u{df}".into()).matches_ignore_case("SS"));

    let pattern = Pattern::Partial {
        starts_with: "\u{c4}PP".into(),
        ends_with: "\u{7d42}\u{7aef}".into(),
        contains: "\u{130}".into(),
    };
    assert!(pattern.matches_ignore_case("\u{e4}pp - i\u{307} - \u{7d42}\u{7aef}"));
    assert!(!pattern.matches_ignore_case("\u{e4}pp - i - \u{7d42}\u{7aef}"));
}

#[test]
fn loading_and_matching_preserve_authored_program_patterns() {
    let config = parse_config("[[rules]]\nname = 'tool'\nprogram.name = 'TOOL.EXE'").unwrap();
    let pattern = config.rules[0].program.name.as_ref().unwrap();
    assert!(pattern.matches_ignore_case("tool.exe"));
    assert_eq!(pattern, &Pattern::Exact("TOOL.EXE".into()));
}
