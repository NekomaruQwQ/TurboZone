/** Positive integer client-area dimensions in physical pixels. */
export type Size = [width: number, height: number];

/**
 * A lowercase TOML-style dotted bare key.
 * Runtime validation applies: /^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$/
 */
export type RuleName = string;

/** Complete configuration accepted by M1; omitted rules means an empty list. */
export interface Config {
    /** Valid rules retain declaration order; broken rules are skipped independently. */
    rules?: Rule[];
}

/** One rule pairing filters with enabled native actions. */
export interface Rule {
    /** Required unique identifier, also used in persistent UI section identity. */
    name: RuleName;
    /** Trimmed display name; omitted or blank values fall back to name. */
    description?: string;
    /** Case-insensitive program filters; all supplied filters are ANDed. */
    program?: ProgramFilter<Pattern>;
    /** Case-sensitive title and inclusive client-size filters. */
    window?: WindowFilter<Pattern>;
    /** Higher priorities win; defaults to zero, with declaration order breaking ties. */
    priority?: number;
    /** Enables client-area centering; defaults to false. */
    move?: boolean;
    /** Disabled when omitted or false; true enables an unbounded selector. */
    resize?: ResizeRule;
}

/** Exact-only resizing or a selector with optional default and bounds. */
export type ResizeRule =
    | boolean
    | {
        /** Sole resize target; no selector is offered. */
        exact: Size;
        default?: never;
        min?: never;
        max?: never;
    }
    | (ResizeLimits & { exact?: never });

/** Selector settings; an empty object enables an unbounded selector. */
export interface ResizeLimits {
    /** Primary target, independent of min/max and not required to be a manifest size. */
    default?: Size;
    /** Inclusive minimum menu choice. */
    min?: Size;
    /** Inclusive maximum menu choice; neither axis may be smaller than min. */
    max?: Size;
}

/** Generic serialized or compiled program filters. */
export interface ProgramFilter<S> {
    /** Case-insensitive program filename. */
    name?: S;
    /** Case-insensitive program path; configured patterns must use forward slashes. */
    path?: S;
}

/** Generic serialized or compiled window filters. */
export interface WindowFilter<S> {
    /** Case-sensitive title. */
    title?: S;
    /** Inclusive minimum controllable client-area size required to match. */
    min?: Size;
    /** Inclusive maximum controllable client-area size required to match. */
    max?: Size;
}

/** Strings match exactly; partial objects AND every nonempty component. */
export type Pattern =
    | string
    | {
        /** Literal prefix; empty means omitted. */
        starts_with?: string;
        /** Literal suffix; empty means omitted. */
        ends_with?: string;
        /** Literal substring; empty means omitted. */
        contains?: string;
    };

// Partial patterns require at least one nonempty component.
// Regexes, globs, exact-string objects, and unknown properties are rejected.

// === Conceptual runtime shape (not serialized configuration) ===

/** Owned pattern plus the predicate selected during validated compilation. */
export interface PatternMatcher {
    pattern: string;
    predicate: (input: string, pattern: string) => boolean;
}

/** A validated rule with defaults resolved and patterns compiled. */
export interface RuntimeRule {
    /** Stable rule identifier. */
    name: RuleName;
    /** Trimmed display name, absent if blank. */
    description?: string;
    /** Compiled program predicates; every predicate must succeed. */
    program_filters: ProgramFilter<PatternMatcher[]>;
    /** Compiled title predicates and validated client-size bounds. */
    window_filters: WindowFilter<PatternMatcher[]>;
    /** Explicit or default matching priority. */
    priority: number;
    /** Whether centering controls are available. */
    relocate: boolean;
    /** Exact-only target, mutually exclusive with resize_limits. */
    resize_exact?: Size;
    /** Selector settings; absent for disabled and exact-only modes. */
    resize_limits?: ResizeLimits;
}

/** Rules stay in source order, without a separate priority index. */
export interface RuntimeConfig {
    rules: RuntimeRule[];
}

/*
Matching and UI semantics:

1. Only snapshots with Ok(WindowDetail) participate in matching and native actions.
2. All filters in a rule are ANDed; highest priority wins, then source order.
3. Program candidates are lowercased once per snapshot for matching.
4. Titles retain their original case; native paths only replace backslashes with forward slashes.
5. A matched section is identified by (rule.name, lowercase program path).
6. An actionless matched rule remains an intentional read-only section.
7. Unmatched windows are discarded normally; failed details are reported to stderr before discarding.
8. Failed snapshots retain handle, title, state, and the first error; the next refresh retries.
9. Size tuples must contain exactly two positive integers; no partial target is accepted.
10. No configurable section abstraction, inheritance, or legacy schema aliases exist.
*/
