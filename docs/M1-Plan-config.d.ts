/** A client-area size in physical pixels. */
export type Size = [width: number, height: number];

/**
 * A lowercase TOML-style dotted bare key.
 *
 * Runtime validation applies: /^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$/
 */
export type RuleName = string;

/** The complete top-level configuration shape accepted by M1. */
export interface Config {
    /** Rules retained in declaration order. */
    rules: Rule[];
}

/** One complete rule accepted by M1. */
export interface Rule {
    /** A required unique identifier used in persistent UI section identity. */
    name: RuleName;
    /** An optional user-facing section name, trimmed while loading. */
    description?: string;

    /**
     * Moving is disabled when omitted or false. `true` and `"center"` both
     * enable centering.
     */
    move?: boolean | "center";

    /**
     * Resizing is disabled when omitted or false. `true` enables the built-in
     * selector without a primary target. A size enables resizing and supplies
     * the primary target.
     */
    resize?:
        | boolean
        | Size
        | {
            /** Whether resize controls are available. */
            enabled: boolean;
            /** An optional target width, used only when target_height is also present. */
            target_width?: number;
            /** An optional target height, used only when target_width is also present. */
            target_height?: number;

            /** An optional minimum width offered by the resize-size selector. */
            min_width?: number;
            /** An optional minimum height offered by the resize-size selector. */
            min_height?: number;
            /** An optional maximum width offered by the resize-size selector. */
            max_width?: number;
            /** An optional maximum height offered by the resize-size selector. */
            max_height?: number;
        };

    /** Constraints used to select the one winning rule for a window. */
    match?: {
        /** Higher priorities win; zero is used when omitted. */
        priority?: number;
        /** Optional executable constraints, ANDed when both are present. */
        executable?: {
            /** A normalized executable-path matcher. */
            path?: StringMatcher;
            /** A case-insensitive executable-filename matcher. */
            name?: StringMatcher;
        };
        /** Optional window constraints, ANDed when several are present. */
        window?: {
            /** A case-sensitive window-title matcher. */
            title?: StringMatcher;
            /** Inclusive minimum client-area size required to match. */
            min_size?: Size;
            /** Inclusive maximum client-area size required to match. */
            max_size?: Size;
        };
    };
}

/**
 * The string-matcher shape accepted by M1. A bare string is always exact. The
 * component form ANDs every supplied component and requires at least one
 * non-empty component.
 */
export type StringMatcher =
    | string
    | { exact: string }
    | {
        starts_with?: string;
        ends_with?: string;
        contains?: string;
    };

/** Future matcher extensions which are deliberately not accepted by M1. */
export type StringMatcherV2 =
    | StringMatcher
    | { regex: string }
    | { glob: string };

// === Validated runtime shape ===

/** A string pattern paired with the predicate selected while loading. */
export interface RuntimeStringMatcher {
    pattern: string;
    predicate: (input: string, pattern: string) => boolean;
}

/** A validated rule with defaults resolved and string matchers compiled. */
export interface RuntimeRule {
    /** The unchanged stable rule identifier. */
    name: RuleName;
    /** The optional trimmed user-facing section name. */
    description?: string;

    /** Fully resolved move behavior. */
    move: false | "center";

    /** Fully resolved resize behavior. */
    resize: {
        /** Whether resize controls are available. */
        enabled: boolean;
        /** A fixed target width, present only together with target_height. */
        target_width?: number;
        /** A fixed target height, present only together with target_width. */
        target_height?: number;
        /** An optional lower bound for offered widths. */
        min_width?: number;
        /** An optional lower bound for offered heights. */
        min_height?: number;
        /** An optional upper bound for offered widths. */
        max_width?: number;
        /** An optional upper bound for offered heights. */
        max_height?: number;
    };

    /** Fully resolved matching behavior. */
    match: {
        /** The explicit or default matching priority. */
        priority: number;
        /** Optional case-insensitive executable predicates. */
        executable?: {
            /** Name predicates which must all succeed. */
            name: RuntimeStringMatcher[];
            /** Path predicates which must all succeed. */
            path: RuntimeStringMatcher[];
        };
        /** Optional case-sensitive window predicates and inclusive size bounds. */
        window?: {
            /** Title predicates which must all succeed. */
            title: RuntimeStringMatcher[];
            /** Inclusive minimum client-area size. */
            min_size?: Size;
            /** Inclusive maximum client-area size. */
            max_size?: Size;
        };
    };
}

/** Runtime rules remain in source order; no secondary match-order structure exists. */
export interface RuntimeConfig {
    rules: RuntimeRule[];
}

/*
Matching and UI semantics:

- M1 accepts the complete Rule and StringMatcher formats above.
- StringMatcherV2 remains reserved for a future implementation.
- Bare matcher strings and explicit exact matchers are exact.
- Executable paths and names compare case-insensitively; titles compare case-sensitively.
- Every configured constraint in a rule is ANDed.
- Every window with an executable path is checked against every rule.
- The highest-priority matching rule wins; source order breaks equal-priority ties.
- Source order also determines normal-page section display order.
- Each matched UI section is identified by (rule.name, normalized lowercase path).
- A matched rule with no enabled actions still produces a read-only section.
- A window with a path but no matching rule becomes unmatched.
- A window without an executable path becomes unknown and is never matched.
- Unmatched and unknown windows appear on one dedicated replacement page with no actions.
- An incomplete one-dimensional resize target emits a warning and is ignored.
- There is no configurable section abstraction and no rule inheritance.
*/
