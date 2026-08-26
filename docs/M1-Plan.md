# M1 Plan

## Configuration contract

TurboZone is rule-driven, without configurable groups or rule inheritance.
[M1-Plan-config.d.ts](M1-Plan-config.d.ts) documents the accepted schema and conceptual runtime
types; [M1-Plan-config.toml](M1-Plan-config.toml) is a validated example.

Each `ConfigRule` contains:

1. A required unique `name`.
2. An optional display `description`, trimmed while loading; blank uses the rule name.
3. Generic `program` and `window` filters containing serialized `Pattern` values.
4. A `priority` that defaults to zero.
5. A `relocate` centering permission that defaults to false.
6. A `resize` rule that defaults to false.

Rule names are plain strings validated as lowercase TOML-style dotted bare keys:

```regex
^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$
```

Names are not trimmed or normalized. Omitted `rules` means an empty rule list.
Unknown properties, old `match_*`, `set_position`, and `set_size` keys, and unsupported
matcher forms are rejected. There are no compatibility aliases.

## Actions and resize modes

`relocate = true` enables centering the controllable client area in its monitor's work area.
False or omission disables centering.

`ResizeRule` is an untagged union:

| Configuration | Runtime exact target | Runtime selector | UI |
| --- | --- | --- | --- |
| Omitted or `resize = false` | None | None | No resize controls |
| `resize = true` or `resize = {}` | None | Unbounded limits | Size selector |
| `resize.exact = [1440, 900]` | That size | None | Exact resize button only |
| `resize = { default = [1440, 900], min = [960, 540], max = [3840, 2400] }` | None | Those settings | Primary button plus selector |

Selector `default`, `min`, and `max` are all optional. Exact and selector fields cannot be
mixed. All sizes are two-element arrays `[width, height]` of positive physical-pixel integers;
incomplete pairs, extra components, and fractional dimensions are rejected.

Minimum bounds must not exceed maximum bounds on either axis. Selector bounds filter built-in
menu choices, not the independently configured primary target. A default or exact target need
not appear in the built-in manifest. A selector with no surviving choices shows an explanatory
message.

Position and resize permissions are independent. A matched rule with neither action remains an
intentional read-only section. Native controls require both a successful window-detail query
and a matching rule that enables the action.

## Patterns and rule selection

A pattern string matches exactly. A partial object supports `starts_with`, `ends_with`, and
`contains`; all nonempty components are ANDed, and at least one must be nonempty. Exact-string
objects, regexes, and globs are not accepted.

Program paths and names compare case-insensitively. Configured patterns are lowercased once
during validation; native candidates are lowercased once per snapshot. Titles stay case-sensitive.
Runtime `ProgramFilter<Vec<PatternMatcher>>` and `WindowFilter<Vec<PatternMatcher>>`
retain absent filters as `None`. Each matcher pairs an owned string with a function pointer.
There are no regex engines, trait objects, or duplicate unchecked compilation paths.

`RuntimeConfig` keeps one source-order vector. For each window with complete details:

1. Scan all rules in source order.
2. AND all configured filters.
3. Replace the winner only when a matching rule has strictly greater priority.
4. Equal priority therefore favors the first rule.

This straightforward scan also preserves source-order UI sections. No additional priority index
is needed for the expected rule/window counts at the 100 ms logic interval.

## Window geometry and snapshots

The core crate owns `WindowInfo<H>`, `WindowDetail`, and `WindowState`. The Windows crate owns
the concrete `WindowHandle` and returns `WindowInfo<WindowHandle>`.

Every snapshot retains its handle, title, and visual state. Its `detail` is either:

1. `Ok(WindowDetail)`: monitor work area, controllable client rectangle, process ID, normalized
   program path, and program filename are all available.
2. `Err(Vec<String>)`: a nonempty list of contextual query failures. Successful subsets of
   detail fields are not retained; this is an all-or-nothing detail boundary.

Independent failures are collected together. Queries whose prerequisites failed do not add
redundant errors. Errors with documented native codes retain those diagnostics; APIs without a
documented last-error contract get an explicit failure message instead of a stale code.

Both rectangles use physical screen coordinates. `monitor_rect` is the work area excluding
taskbars, not the full monitor extent. `content_rect` is the live client rectangle for normal
windows, or inferred restored client geometry for minimized/maximized windows.

Restored placement uses standard frame offsets at the window's DPI. Workspace coordinates are
converted to screen coordinates for ordinary top-level windows; tool-window placement already
uses screen coordinates. See the
[Win32 placement contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement)
and [DPI-aware frame calculation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-adjustwindowrectexfordpi).
Custom non-client layouts and wrapped menus are not fully inferable from window styles; restored
client geometry retains this limitation of the standard-frame approach.

`WindowEnumerator` caches monitor-query successes and failures by monitor handle within one
snapshot. The cache clears before each new enumeration, so work-area changes and monitor
removal are not carried across refreshes. Native actions query current geometry separately.
Failed details are retried by the next scheduled snapshot, without extra retry loops.

## Size filters

`window.min` and `window.max` filter rule eligibility using `content_rect.size`.
They are independent of `resize.min` and `resize.max`, which filter selector choices.

Bounds are inclusive on both axes, positive when present, and otherwise unrestricted. Normal
windows use live client size; minimized/maximized windows use restored client size. Any
detail-query failure excludes the whole snapshot from matching, even for unfiltered rules.

Matching is reevaluated on every snapshot, so moving, resizing, or query recovery may change a
window's winning rule.

## Classification and UI

Every eligible snapshot moves into exactly one destination:

```text
WindowInfo
    Err(errors) -> failed_windows
    Ok(detail), no winning rule -> unmatched_windows
    Ok(detail), winning rule -> (rule, program path) section
```

`SectionedWindows` owns the complete, disjoint classification without cloning snapshots.
Sections are ordered by rule source order, then lowercase program path. Persistent section
identity is `(rule.name, normalized lowercase program path)`; the rule index is snapshot-local.

Windows from the same program matching the same rule share a section. Different paths or
winning rules form separate sections. Actionless matched rules still appear on the sections page.

The separate diagnostics page has two categories, both without native controls:

1. **Unmatched windows:** complete metadata useful for authoring a rule.
2. **Details unavailable:** retained title, handle, state, and explicit error messages.

A geometry failure also removes program identity from usable details, so that window moves
out of its matched section until a later snapshot succeeds. Old details are never substituted.
Diagnostic status uses text as well as color.

## Native paths and actions

Core `normalize_native_path` lexically resolves native `.` and `..` components without
filesystem access, falls back to the original path if normalization fails, converts unusual
non-Unicode paths lossily, and replaces backslashes with forward slashes. Display casing is
preserved. Configured patterns are not path-normalized, and backslashes in them are rejected.

Centering aligns the client-area center with the monitor work-area center. Resizing preserves
the integer client-area center, including odd-sized targets. Actions preserve activation, z-order,
and normal/maximized/minimized state; restored actions update placement rather than restoring
the window. Oversized targets that cannot fit native dimensions return errors.

Individual and section actions capture handles from the rendered snapshot and execute on the
next logic tick. A snapshot is not an atomic OS transaction: windows can disappear or change
between queries and actions, so native actions remain fallible.

If top-level enumeration fails, both matched and diagnostic snapshots are cleared and the
enumeration error is shown. Per-window detail failures do not invalidate other windows.

## Source layout

1. `turbozone-core/data/`: serialized config, generic patterns/filters, runtime rules,
   platform-independent window snapshots, and native-path normalization.
2. `turbozone-core/config.rs`: validated compilation and typed configuration errors.
3. `turbozone-core/manifest.rs`: built-in Euclid resize choices.
4. `turbozone-windows/`: native handle, cached enumeration, geometry queries, and actions.
5. `turbozone/configuration.rs`: program-relative configuration loading and error presentation.
6. `turbozone/data.rs`: disjoint classification and page state.
7. `turbozone/app.rs`: snapshot timing and queued native actions.
8. `turbozone/ui/view.rs`: matched sections, action controls, and diagnostic rendering.

## Verification

Tests cover schema validation/serialization, all resize modes, selector bounds, exact and partial
matching, case sensitivity, priority ties, size filters, path normalization, complete/failed
classification, recovery, monitor caching, restored geometry, and action availability.

The TOML example is parsed and validated by a core regression test. Verification uses release-mode
workspace tests and Clippy with warnings denied. Formatting is manual; no formatter is run.
