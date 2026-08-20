# M1 Plan

## Configuration contract

TurboZone is rule-driven. There is no configurable or runtime group abstraction, and rules do
not inherit from other rules. M1 accepts the complete `Rule` and `StringMatcher` formats in
`docs/M1-Plan-config.d.ts`; only `StringMatcherV2` regex and glob forms remain reserved.

The example configuration is `docs/M1-Plan-config.toml`.

Each rule contains:

- A required unique `name`.
- An optional display `description`.
- Independent move and resize permissions.
- Optional executable, title, and client-size constraints.
- A matching priority which defaults to zero.

Rule names remain plain strings at runtime. Loading validates them as lowercase TOML-style
dotted bare keys:

```regex
^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$
```

Names are neither trimmed nor normalized. Descriptions are trimmed but otherwise accepted,
including an empty trimmed value.

Unknown properties and unsupported matcher forms are rejected instead of ignored. No
compatibility or migration logic for the previous named-container configuration is included.

## Action shorthand

Move behavior:

- Omitted `move` and `move = false` disable move controls.
- `move = true` and `move = "center"` enable centering.
- Centering is the only M1 move target.

Resize behavior:

- Omitted `resize` and `resize = false` disable resize controls.
- `resize = true` enables the built-in size selector without a primary target.
- `resize = [width, height]` enables resizing and configures the primary target.
- A resize object preserves its `enabled`, target, and selector-limit properties.
- Target dimensions are used only when both width and height are present.
- Exactly one target dimension emits a warning and both runtime target dimensions are ignored.
- Every provided dimension must be positive.
- Each selector minimum must not exceed its corresponding maximum.
- Selector limits filter built-in choices but do not constrain the configured primary target.

Move and resize permissions are independent. A matched rule with neither action enabled remains
useful as an intentional read-only section.

This is a whitelist safety model: a window receives native controls only through a matched rule
whose corresponding action is enabled.

## String matching

A bare string and `{ exact = "..." }` both require an exact match. The component form supports
`starts_with`, `ends_with`, and `contains`; every supplied component is ANDed. It must contain at
least one non-empty component.

Executable paths and filenames compare case-insensitively. Their configured patterns are
lowercased once during loading, while each native candidate is lowercased once per snapshot.
Window titles remain case-sensitive.

Each runtime string predicate is represented directly by an owned pattern and a function
pointer selected during loading. No regex, glob, trait object, or dynamic matcher allocation is
included in M1.

## Rule selection

`RuntimeConfig` stores one source-order `Vec<RuntimeRule>`. There is no secondary priority or
match-order structure.

For each eligible window:

1. Scan every runtime rule in source order.
2. AND every configured constraint within the rule.
3. Keep a matching rule only when its priority is strictly higher than the current winner.
4. The first rule therefore wins among equal priorities.

Scanning every rule keeps runtime editing and UI ordering direct. The expected number of rules
and windows makes this work negligible at the 100 ms logic interval; optimization requires
measurement rather than an additional indexing structure.

## Window-size matching

`match.window.min_size` and `match.window.max_size` filter rule eligibility. They are separate
from `resize.min_*` and `resize.max_*`, which filter selector choices.

- Bounds describe the controllable client area in physical pixels.
- Normal windows use their live client size.
- Maximized and minimized windows use their restored client size.
- Comparisons are inclusive on both dimensions.
- Omitted bounds are unconstrained.
- Every configured dimension must be positive.
- Each minimum dimension must not exceed its corresponding maximum dimension.
- A window whose client size cannot be queried does not match a size-constrained rule.

Matching is reevaluated from every native snapshot. Moving or resizing may therefore change a
window's winning rule on the next logic tick.

## Window classification and sections

Every native snapshot moves into exactly one destination:

```text
WindowInfo
    path unavailable -> unknown_windows
    path available, no winning rule -> unmatched_windows
    path available, winning rule -> (rule, path) section
```

`SectionedWindows` owns:

- Source-ordered matched sections.
- Known-path unmatched windows.
- Path-unavailable unknown windows.

A normal UI section is identified persistently by `(rule.name, normalized lowercase executable
path)`. The section stores a snapshot-local rule index for direct access to the complete
`RuntimeRule`; the index is never persistent and sections must be rebuilt together with any
runtime rule edit.

Consequences:

- Windows from the same executable which match the same rule share a section.
- One rule matching different executable paths produces separate sections.
- Different rules matching the same executable path produce separate sections.
- Rule source order determines section order; path order is deterministic within each rule.
- A matched actionless rule creates a read-only normal section.
- Missing executable paths never use a filename or process-ID fallback and never reach matching.

Section construction consumes `Vec<WindowInfo>`. It borrows fields while matching and then moves
each complete snapshot into its destination. The previous generic candidate wrapper and its
duplicated metadata are removed; no `WindowInfo` clone is required.

## Diagnostic replacement page

The UI has a normal sections page and a diagnostic replacement page. The diagnostic page shows:

- `unmatched_windows`, which have executable paths but match no rule.
- `unknown_windows`, which lack executable paths and were rejected from matching.

Both categories show the metadata needed to author a rule and expose no native actions. They do
not appear beneath matched sections.

An unmatched window remains semantically different from a window matched by an actionless rule:
the latter appears intentionally on the normal page.

## Native path handling

Only paths originating from Win32 are normalized. Configuration patterns are consumed exactly as
written.

The native pipeline is:

```rust
path
    .normalize_lexically()
    .to_string_lossy()
    .replace('\\', "/")
```

This resolves lexical `.` and `..` components without filesystem access, converts native
separators, accepts lossy conversion for unusual non-Unicode paths, and preserves the casing
reported by Windows. Matching and section identity perform case conversion separately.

A backslash in any configured executable-path pattern is a validation error.

## Native actions

Centering preserves window size, activation, z-order, and normal/maximized/minimized state.
Resizing preserves the window center and visual state and targets the controllable client area.

Section-level actions capture the handles from the currently rendered snapshot. Individual
actions capture one handle. Side effects remain queued until the next logic tick.

If native enumeration fails, matched and diagnostic snapshot state is cleared so stale handles
do not remain actionable. Resize history remains out of scope.

## Source layout

### `turbozone-core`

- `data.rs`: serialized configuration and validated runtime rule types.
- `config.rs`: validation, normalization, matcher compilation, and typed `thiserror` failures.
- `manifest.rs`: Euclid geometry choices used by the resize selector.
- Source-order rule selection and platform-neutral matching.

### `turbozone-windows`

- Concrete `WindowInfo`, `WindowHandle`, and `WindowState` snapshots.
- Win32 enumeration and manipulation.
- Native executable-path normalization and display metadata.
- Centering and resizing.

### `turbozone`

- `configuration.rs`: executable-relative config discovery, TOML parsing, and error presentation.
- `data.rs`: concrete `WindowSection`, `SectionedWindows`, and page state.
- `app.rs`: snapshot timing and queued native actions.
- `ui/view.rs`: matched sections and the diagnostic replacement page.

## Validation and verification

Tests cover:

- Rule-name grammar, uniqueness, and description trimming.
- Boolean, string, tuple, and complete action forms.
- Incomplete resize-target normalization.
- Positive dimensions and ordered bounds.
- Bare, exact, and component matchers.
- Matcher AND semantics and executable/title case behavior.
- Rejection of backslashes, empty components, unknown properties, regexes, and globs.
- Higher-priority selection and source-order tie breaking.
- Inclusive window-size matching and missing-size behavior.
- Selector-limit filtering.
- Native path normalization.

Final verification uses release-mode workspace tests and Clippy with warnings denied. Automated
formatters are not run; source is formatted manually to the project style.
