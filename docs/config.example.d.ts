/** Integer client-area dimensions from 1 through 16,384 physical pixels. */
export type Size = [width: number, height: number];

/**
 * A lowercase TOML-style dotted bare key.
 * Runtime validation applies: /^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$/
 */
export type RuleName = string;

/** Complete TurboZone configuration; omitted rules means an empty list. */
export interface Config {
    /** Rules retain declaration order; any invalid rule rejects the complete configuration. */
    rules?: Rule[];
}

/** One rule pairing filters with enabled native actions. */
export interface Rule {
    /** Required unique identifier, also used in persistent UI section identity. */
    name: RuleName;
    /** Authored description; the display-name query trims and falls back to name when blank. */
    description?: string;
    /** Case-insensitive program filters; all supplied filters are ANDed. */
    program?: ProgramFilter;
    /** Case-sensitive title and inclusive client-size filters. */
    window?: WindowFilter;
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
    | Size
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
    /** Primary target; warns and is unavailable outside min/max. Need not be a manifest size. */
    default?: Size;
    /** Inclusive minimum for the default and menu choices. */
    min?: Size;
    /** Inclusive maximum for the default and menu choices; neither axis may be smaller than min. */
    max?: Size;
}

/** Program filters shared by serialization and direct matching. */
export interface ProgramFilter {
    /** Case-insensitive program filename. */
    name?: Pattern;
    /** Case-insensitive program path; configured patterns must use forward slashes. */
    path?: Pattern;
}

/** Window filters shared by serialization and direct matching. */
export interface WindowFilter {
    /** Case-sensitive title. */
    title?: Pattern;
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

// Rust's parser returns Config after non-mutating verification; Engine owns Rule[]
// in source order and exposes borrowed rules. No separate runtime rule model exists.
// ResizeRule queries derive the primary target and optional selector settings on demand.

/*
Matching and UI semantics:

1. Only snapshots with Ok(WindowDetail) participate in matching and native actions.
2. All filters in a rule are ANDed; highest priority wins, then source order.
3. Program patterns and candidates are lowercased at match time without mutating either.
4. Titles retain their original case; native paths only replace backslashes with forward slashes.
5. A matched section is identified by (rule.name, lowercase program path).
6. An actionless matched rule remains an intentional group with disabled-action labels.
7. Unmatched windows are discarded normally; failed details are reported to stderr before discarding.
8. Failed snapshots retain handle, title, state, and the first error; the next refresh retries.
9. Size tuples must contain exactly two integers from 1 through 16,384; no partial target is accepted.
10. No configurable section abstraction, inheritance, or legacy schema aliases exist.
*/
