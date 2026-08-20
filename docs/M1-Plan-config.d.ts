/** A client-area size in physical pixels. */
export type Size = [width: number, height: number];

/** The planned complete configuration shape. */
export interface Rule {
    /** A required unique identifier used as part of the persistent UI section identity. */
    name: string;
    /** An optional user-facing section name; the rule name is used when omitted. */
    description?: string;

    /**
     * Moving is disabled when omitted or false. The string form enables the
     * selected target, while the object form is reserved for the complete format.
     */
    move?:
        | false
        | "center"
        | {
            /** Whether move controls are available. */
            enabled: boolean;
            /** The move target; center is the only target currently planned. */
            target: "center";
        };

    /**
     * Resizing is disabled when omitted or false. A size enables resizing and
     * supplies both target dimensions. The object form is reserved for the
     * complete format and can target only one dimension.
     */
    resize?:
        | false
        | Size
        | {
            /** Whether resize controls are available. */
            enabled: boolean;
            /** An optional target width; omission preserves each window's width. */
            target_width?: number;
            /** An optional target height; omission preserves each window's height. */
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
        /** Higher priorities match first; zero is used when omitted. */
        priority?: number;
        /** Optional executable constraints, ANDed when both are present. */
        executable?: {
            /** A normalized executable-path matcher. */
            path?: StringMatcher;
            /** An exact executable filename, compared case-insensitively. */
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
 * The planned complete string-matcher shape. A bare string is always exact.
 * Exact, regex, and glob are exclusive forms. The component form ANDs every
 * supplied component and must contain at least one component.
 */
export type StringMatcher =
    | string
    | { exact: string }
    | { regex: string }
    | { glob: string }
    | {
        starts_with?: string;
        ends_with?: string;
        contains?: string;
    };

// === M1-supported serialized subset ===

/**
 * The initial format accepted by M1. Every accepted value has the same meaning
 * in Rule, allowing later releases to add the complete forms without migration.
 */
export interface SimpleRule {
    /** A required unique identifier used as part of the UI section identity. */
    name: string;
    /** An optional user-facing section name; the rule name is used when omitted. */
    description?: string;

    /** Omitted or false disables moving; center enables centering. */
    move?: false | "center";
    /** Omitted or false disables resizing; a size enables it and supplies the target. */
    resize?: false | Size;

    /** Constraints used to select the one winning rule for a window. */
    match?: {
        /** Higher priorities match first; zero is used when omitted. */
        priority?: number;
        /** Optional exact executable constraints, ANDed when both are present. */
        executable?: {
            /** An exact normalized path, compared case-insensitively. */
            path?: string;
            /** An exact executable filename, compared case-insensitively. */
            name?: string;
        };
        /** Optional exact title and inclusive client-size constraints. */
        window?: {
            /** An exact case-sensitive title. */
            title?: string;
            /** Inclusive minimum client-area size required to match. */
            min_size?: Size;
            /** Inclusive maximum client-area size required to match. */
            max_size?: Size;
        };
    };
}

// === Validated runtime shape ===

/** A string matcher compiled once while loading configuration. */
export type CompiledStringMatcher = (candidate: string) => boolean;

/** A validated rule with defaults resolved and string matchers compiled. */
export interface CompiledRule {
    /** The stable unique rule identifier. */
    name: string;
    /** The optional user-facing section name. */
    description?: string;
    /** The zero-based source position used for deterministic ties and display order. */
    source_order: number;

    /** Fully resolved move behavior. */
    move: {
        /** Whether move controls are available. */
        enabled: boolean;
        /** The resolved move target. */
        target: "center";
    };

    /** Fully resolved resize behavior. */
    resize: {
        /** Whether resize controls are available. */
        enabled: boolean;
        /** An optional fixed target width. */
        target_width?: number;
        /** An optional fixed target height. */
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
        /** Optional executable matchers; filename matching may compile to a path suffix. */
        executable?: {
            /** Path predicates which must all succeed. */
            path: CompiledStringMatcher[];
        };
        /** Optional window predicates and inclusive size bounds. */
        window?: {
            /** Title predicates which must all succeed. */
            title: CompiledStringMatcher[];
            /** Inclusive minimum client-area size. */
            min_size?: Size;
            /** Inclusive maximum client-area size. */
            max_size?: Size;
        };
    };
}

/*
Matching and UI semantics:

- A bare matcher string is exact in both the M1 and complete formats.
- Executable paths and names compare case-insensitively; titles compare case-sensitively.
- Every configured constraint in a rule is ANDed.
- Higher-priority rules match first. Source order breaks equal-priority ties.
- Source order, not priority, determines normal-page section display order.
- Each matched UI section is identified by (rule.name, executable identity).
- A matched rule with no enabled actions still produces a read-only section.
- Windows which match no rule appear on a dedicated replacement page with no actions.
- There is no configurable group abstraction and no rule inheritance.
- M1 accepts only SimpleRule. Complete-only forms must be rejected rather than ignored.
*/
