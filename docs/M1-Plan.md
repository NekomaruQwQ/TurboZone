# M1 Plan

## Proposal of Change v4

### Configuration model

TurboZone is rule-driven. There is no configurable or runtime group abstraction, and rules do
not inherit from other rules.

The complete planned configuration shape is documented by `docs/config.d.ts`. M1 implements
only its `SimpleRule` subset:

```ts
interface SimpleRule {
    name: string;
    description?: string;
    move?: false | "center";
    resize?: false | [number, number];
    match?: {
        priority?: number;
        executable?: {
            path?: string;
            name?: string;
        };
        window?: {
            title?: string;
            min_size?: [number, number];
            max_size?: [number, number];
        };
    };
}
```

Every value accepted by `SimpleRule` has the same meaning in the complete `Rule` format. M1
rejects complete-only action objects and advanced string matchers rather than ignoring them.
This allows the format to expand later without changing the meaning of an existing M1 file.

An M1 rule can be written as:

```toml
[[rules]]
name = "vscode.main"
description = "VS Code (Main Projects)"
move = "center"
resize = [1440, 900]

match.priority = 10
match.executable.name = "Code.exe"
match.window.min_size = [640, 480]
match.window.max_size = [7680, 4320]
```

Configuration semantics:

- `name` is a required, non-empty, unique, stable rule identifier.
- `description` is the optional user-facing section name; `name` is displayed when it is absent.
- Missing or explicit `false` actions are disabled.
- A rule with no enabled actions remains useful as an intentional read-only match.
- Missing match fields are unconstrained, so a rule without `match` is a catch-all.
- Bare strings are always exact matches.
- The complete format may later add action objects, partial resize targets, resize-selector
  limits, regexes, globs, and component string matchers.
- No compatibility or migration logic for the previous configuration model is included.

This is a whitelist safety model: windows receive native actions only through an explicitly
matched rule whose corresponding action is enabled.

### Rule selection

Every rule constraint is ANDed. Each window is assigned to at most one rule:

1. Rules with higher `match.priority` are tested first.
2. The default priority is zero.
3. Source order breaks ties between equal priorities.
4. The first matching rule wins.

Priority affects matching only. Source order determines section display order on the normal
page, followed by deterministic executable ordering within each rule.

Executable paths and filenames compare case-insensitively. Window titles remain
case-sensitive. In M1, all three are exact string matches.

### Window-size matching

`match.window.min_size` and `match.window.max_size` filter which windows a rule accepts. They
are distinct from the complete format's `resize.min_*` and `resize.max_*` properties, which
filter the choices offered by the resize-size selector.

M1 window-size matching uses these semantics:

- Bounds describe the controllable client area in physical pixels.
- Normal windows use their live client size.
- Maximized and minimized windows use their restored client size.
- Minimum and maximum comparisons are inclusive on both dimensions.
- `min_size = [width, height]` requires both actual dimensions to be at least the bound.
- `max_size = [width, height]` requires both actual dimensions to be at most the bound.
- Omitted bounds are unconstrained; sentinel values such as `[0, 0]` are not used.
- Every configured dimension must be positive.
- Each minimum dimension must not exceed the corresponding maximum dimension.
- A window whose client size cannot be queried does not match a size-constrained rule.

Matching is reevaluated from each native snapshot. A move or resize may therefore cause a
window to enter a different section or become unmatched on the next logic tick. This is
intentional and supports both protective filters and corrective rules.

### UI sections

The normal page contains one section for each resolved `(rule name, executable identity)` pair.
There is no additional named container:

```text
window
    -> highest-priority matching rule
    -> (rule name, executable identity)
    -> UI section
```

The normalized executable path is the primary executable identity. Paths differing only in
case resolve to the same section. If Windows cannot provide a path, the process ID is used as a
safe fallback so unrelated inaccessible executables do not collapse into one section.

Consequences:

- Windows from the same executable which match the same rule share a section.
- The same rule matching different executable paths produces separate sections.
- Different rules matching the same executable path produce separate sections.
- A section uses its rule's description, action availability, and configured resize target.
- A matched rule with no enabled actions produces a read-only section.

The persistent section identity uses the stable rule name and executable identity, never the
display description.

### Unmatched-windows page

Windows which match no rule are collected separately. They do not receive a synthetic default
rule and do not appear below the matched sections.

The UI exposes them on a dedicated page which replaces the normal matched-sections page while
open. This page is diagnostic and provides no move, resize, or other native action. It displays
the metadata needed to author a rule, including title, normalized executable path, executable
name, process ID, and controllable client size when available.

An unmatched window is semantically different from a window matched by an actionless rule:
the latter appears as an intentional read-only section on the normal page.

### Path normalization

Only paths originating from Win32 are normalized. Configuration values are consumed exactly as
written.

The native-path pipeline is:

```rust
path
    .normalize_lexically()
    .to_string_lossy()
    .replace('\\', "/")
```

This intentionally:

- Resolves lexical `.` and `..` components.
- Avoids filesystem access and canonicalization.
- Converts Win32 backslashes to config-style slashes.
- Accepts lossy conversion for unusual non-Unicode executable paths.
- Preserves the casing returned by Win32; comparison handles case insensitivity.

Configured paths are not normalized or silently corrected. A backslash in
`match.executable.path` is a validation error, making the forward-slash contract visible.

### Move behavior

- Missing `move` and `move = false` disable move controls.
- `move = "center"` enables centering for the section and its individual windows.
- Centering preserves window size, activation, z-order, and normal/maximized/minimized state.
- The complete format reserves an explicit move object for future targets; M1 rejects it.

### Resize behavior

- Missing `resize` and `resize = false` disable resize controls.
- `resize = [width, height]` enables resizing and configures the primary client-area target.
- Both target dimensions must be positive.
- The built-in size selector remains available as a one-shot alternative.
- Selecting an alternative size does not mutate the configured primary target.
- M1 does not support resize-without-a-primary-target, partial target dimensions, or selector
  limits. Those are reserved by the complete format.
- Resizing preserves the window center and normal/maximized/minimized state.

Resize history remains out of scope. No history file is created or read in M1.

### Runtime representation

Configuration is validated and compiled once during loading:

```text
serialized rules     -> Vec<SimpleRule>
compiled rules       -> Vec<CompiledRule>
matched sections     -> ordered Vec<WindowSection<T>>
unmatched windows    -> Vec<T>
```

String comparison preparation and rule-priority ordering happen during compilation rather than
on every logic tick. Section construction retains source order for UI presentation while using
the stable `(rule name, executable identity)` key for aggregation and persistent UI state.

### Euclid in `turbozone-core`

`turbozone-core` uses euclid for all geometry:

```rust
pub type Size2D<i32> = euclid::default::Size2D<i32>;
```

TOML sizes remain two-element arrays:

```toml
resize = [1440, 900]
match.window.min_size = [640, 480]
```

A small serde adapter converts `[i32; 2]` to and from `Size2D<i32>`. Configuration validation
rejects invalid geometry before it becomes runtime state.

### Crate responsibilities

- `turbozone-core`
  - Euclid geometry.
  - Serialized, validated, and compiled rule types.
  - Rule validation, priority ordering, and exact matching.
  - Platform-neutral section construction and unmatched-window classification.
  - Generic `WindowSection<T>`.
  - No native calls or Windows types.

- `turbozone-windows`
  - Win32 enumeration and manipulation.
  - Native path normalization.
  - Executable metadata and fallback executable identity.
  - HWND/PID snapshots.
  - Centering and resizing.

- `turbozone`
  - `TurboZone.exe`.
  - `<exeName>.config.toml` discovery.
  - TOML loading and error presentation.
  - Matched-sections page and replacement unmatched-windows page.
  - Queued native actions.

### Important tests

The core and app test suites will cover:

- Empty and duplicate rule names.
- Omitted and explicit-false actions defaulting to disabled.
- Valid move and resize shorthand forms.
- Rejection of complete-only forms during M1.
- Catch-all rules and actionless read-only rules.
- Higher-priority selection and source-order tie breaking.
- Rule fields being ANDed.
- Case-insensitive exact path and filename matching.
- Case-sensitive exact title matching.
- Backslash rejection in configured paths.
- Win32 path normalization, including `.` and `..`.
- Inclusive minimum and maximum client-size matching.
- Size-query failure preventing size-constrained matches.
- Invalid dimensions and minimum-greater-than-maximum bounds.
- Same-rule/same-executable section aggregation.
- Same-rule/different-executable section separation.
- Different-rule/same-executable section separation.
- Process-ID fallback when the executable path is unavailable.
- Unmatched windows remaining outside normal sections and exposing no actions.
- Matched actionless windows appearing as read-only normal sections.
- Alternative UI sizes not changing the configured resize target.
- Section membership updating after a size-changing action.

### Implementation order

1. Replace the named-container and executable-policy configuration with `SimpleRule` parsing and
   validation.
2. Compile defaults, priority order, exact matchers, and client-size bounds into `CompiledRule`.
3. Replace the old runtime aggregation with section construction keyed by rule name and
   executable identity.
4. Separate unmatched windows from normal sections and implement the replacement diagnostic page.
5. Connect rule-scoped move and resize controls to existing queued native actions.
6. Update configuration errors and UI text to use rule/section terminology consistently.
7. Add the specified core, Windows, and UI tests.
8. Run release-mode workspace tests and Clippy, then perform visual QA without automated
   formatting.
