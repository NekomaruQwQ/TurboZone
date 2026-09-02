# TurboZone design

## Configuration contract

TurboZone is rule-driven, without configurable groups or rule inheritance.
The Rust config types define the accepted structure. Startup generates a schema beside the
selected config for [editor completion and validation](Schema.md).
[config.example.d.ts](config.example.d.ts) illustrates configuration and conceptual runtime
types; [config.example.toml](config.example.toml) is a validated example.

Each `Rule` contains:

1. A required unique `name`.
2. An optional display `description`, trimmed while loading; blank uses the rule name.
3. Generic `program` and `window` filters containing serialized `Pattern` values.
4. A `priority` that defaults to zero.
5. A `move` centering permission that defaults to false (`relocate` in Rust).
6. A `resize` rule that defaults to false.

Rule names are plain strings validated as lowercase TOML-style dotted bare keys:

```regex
^[a-z0-9_-]+(?:\.[a-z0-9_-]+)*$
```

Names are not trimmed or normalized. Omitted `rules` means an empty rule list.
Unknown properties, old `match_*`, `set_position`, and `set_size` keys, and unsupported
matcher forms are rejected. There are no compatibility aliases.

`turbozone-core::parse_config()` checks the document envelope, then deserializes and compiles
rules independently. Invalid TOML or top-level structure is fatal. Invalid individual rules are
skipped as a whole, retaining the valid rules in declaration order. Diagnostics identify the
original array position solely as a source location, never as runtime identity. Only the first
valid occurrence reserves a name; later duplicates
are skipped. An empty document, or a document with no usable rules, is valid.

## Startup and diagnostics

1. Require `--config <FILE>` or `TURBOZONE_CONFIG`; the CLI takes precedence. Empty values are
   rejected. Relative paths resolve against the working directory; there is no implicit fallback.
2. Refresh `config_path.with_extension("schema.json")` from `Config::schema()`. A write failure
   is a warning, not a reason to reject an otherwise usable config.
3. Leave existing config bytes untouched, including comments, schema directives, BOM, and line
   endings. Create a missing file exclusively with only a relative `#:schema` comment. Parent
   directories must already exist. Concurrent creation never authorizes overwriting the file.
4. Parse and compile via `turbozone-core`, log rejected rules and the loaded/skipped counts,
   then launch the UI with a `RuntimeConfig`.

Unreadable config, creation failure, and malformed documents exit nonzero before the UI opens.
The generated schema is replaceable, but user-authored config is never rewritten automatically;
there is no temporary-file replacement workflow.

TurboZone assumes a terminal is available. The Windows binary installs
`pretty_env_logger::init()` without a custom filter builder; `RUST_LOG` controls filtering.
Diagnostics omit TOML source excerpts. Per-window and action failures include the handle,
contextual error chain, and available title or executable path. Unchanged periodic failures are
suppressed until they change, recover, or disappear and recur. Enumeration failures are also
deduplicated. No diagnostic UI state is kept.

## Actions and resize modes

`move = true` enables centering the controllable client area in its monitor's work area.
False or omission disables centering.

`ResizeRule` is an untagged union:

| Configuration | Runtime exact target | Runtime selector | UI |
| --- | --- | --- | --- |
| Omitted or `resize = false` | None | None | No resize controls |
| `resize = true` or `resize = {}` | None | Unbounded limits | Size selector |
| `resize = [1440, 900]` | None | That default with unbounded limits | Primary button plus selector |
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
retain absent filters as `None`. Each matcher pairs an immutable `SmolStr` with a function pointer.
`Pattern::to_matchers()` compiles literal case-sensitive predicates; the config compiler supplies
normalized program patterns and rejects empty partials before calling it. No regex engine or
trait objects are involved.

`RuntimeConfig` keeps one source-order vector but exposes rules by stable name rather than array
position. For each window with complete details:

1. Scan all rules in source order.
2. AND all configured filters.
3. Replace the winner only when a matching rule has strictly greater priority.
4. Equal priority therefore favors the first rule.

`matching_rule_name()` returns the winning name and `rule(name)` resolves it. This straightforward
scan preserves source-order UI sections without allowing array positions to escape as identity.
No additional priority index is needed for the expected rule/window counts at the 10 Hz logic rate.

## Window geometry and snapshots

The core crate owns `WindowInfo<H>`, `WindowDetail`, `WindowState`, and the generic `Backend`
contract. The Windows crate owns the concrete `Handle<HWND>` and returns
`WindowInfo<Handle<HWND>>`. Owned UTF-8 text crossing TurboZone API boundaries uses `SmolStr`,
including serialized config values, diagnostics, startup file contents, runtime names, titles,
paths, and matcher literals. Standard-library and serializer helpers may construct temporary
buffers before conversion; persisted bytes and external string formats remain unchanged.

Every snapshot retains its handle, title, and visual state. Its `detail` is either:

1. `Ok(WindowDetail)`: monitor work area, controllable client rectangle, process ID, normalized
   program path, and program filename are all available.
2. `Err(anyhow::Error)`: the first contextual query failure, preserving its original cause.
   Successful subsets of detail fields are not retained; this is an all-or-nothing detail boundary.

Detail queries stop at the first failure. Errors with documented native codes retain those
causes; APIs without a documented last-error contract get an explicit failure message instead
of a stale code.

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

The Windows `Backend` is stateless. Each `Backend::snapshot()` call owns a monitor cache shared by
all window-detail queries in that snapshot, including successful and failed monitor results. The
cache is discarded when the call returns, so work-area changes and monitor removal are not carried
across refreshes. Native actions query current geometry separately. Failed details are retried by
the next scheduled snapshot, without extra retry loops.

## Size filters

`window.min` and `window.max` filter rule eligibility using `content_rect.size`.
They are independent of `resize.min` and `resize.max`, which filter selector choices.

Bounds are inclusive on both axes, positive when present, and otherwise unrestricted. Normal
windows use live client size; minimized/maximized windows use restored client size. Any
detail-query failure excludes the whole snapshot from matching, even for unfiltered rules.

Matching is reevaluated on every snapshot, so moving, resizing, or query recovery may change a
window's winning rule.

## Classification and UI

Snapshots are logged before classification, then only complete matches are retained:

```text
WindowInfo
    Err(error) -> log first/changed failure, discard snapshot
    Ok(detail), no winning rule -> discard normally
    Ok(detail), winning rule -> (rule, program path) section
```

`group_windows()` moves matched snapshots into `Vec<WindowSection<H>>` without cloning them.
Sections are ordered by valid rule source order, then lowercase program path. Persistent section
identity is `(rule.name, lowercase program path)`; rule array indices are never retained.

Windows from the same program matching the same rule share a section. Different paths or
winning rules form separate sections. Actionless matched rules still appear. The UI keeps section
and window actions, program paths, process IDs, titles, visual states, and size highlighting.

A geometry failure removes the whole snapshot from matching until a later snapshot succeeds.
Old details are never substituted. Unmatched windows are normal, with no warning or diagnostic
bucket. An empty matched list shows the ordinary empty state. There is no diagnostics page,
page navigation, error card, diagnostic count, or configuration-path heading.

## Native paths and actions

Windows is assumed to return already normalized paths with backslash separators. The Windows
adapter converts non-Unicode paths lossily and replaces backslashes with forward slashes,
without lexical normalization or filesystem canonicalization. Display casing is preserved.
Configured patterns are not path-normalized, and backslashes in them are rejected.

Centering aligns the client-area center with the monitor work-area center. Resizing preserves
the integer client-area center, including odd-sized targets. Actions preserve activation, z-order,
and normal/maximized/minimized state; restored actions update placement rather than restoring
the window. Oversized targets that cannot fit native dimensions return errors.

Individual actions capture one handle from the rendered snapshot. Section controls append one
action per eligible handle. Core's non-exhaustive `WindowAction<H>` currently contains
`Resize(H, size)` and `MoveToCenter(H)`. The generic engine drains actions in order by calling
`Backend::perform(action)`, then refreshes through `Backend::snapshot()`. The backend owns variant
dispatch and deliberately panics on a future unsupported variant rather than silently doing nothing.

The eframe app records the last completed logic tick and calls the engine immediately at startup,
when an action is pending, or at the core-owned 10 Hz cadence. A snapshot is not an atomic OS
transaction: windows can disappear or change between queries and actions, so native actions
remain fallible.

If top-level enumeration fails, stale matched sections are cleared and the enumeration error
is logged. Per-window detail failures do not invalidate other windows. User-requested action
failures are always logged and do not abort actions for the remaining targets.

## Source layout

1. `turbozone-core/src/`: CLI shape, serialized config and schema, validation, runtime matching,
   backend contract, action queue, snapshot lifecycle, stable sections, log deduplication, window
   models, and product cadence constants. It performs no filesystem I/O and does not install a logger.
2. `turbozone-ui/src/`: generic `App<B>`, egui presentation, and standard-library config/schema
   filesystem operations. It has no dependency on the Windows crate.
3. `turbozone-windows/src/`: native handles, stateless snapshot adaptation with call-local monitor
   caching, geometry queries, `Backend`, Windows font setup, logger initialization, and the
   `turbozone.exe` entry point.

## Verification

Integration tests cover schema/serialization, partial rule recovery, all resize modes, selector
bounds, exact and partial matching, case sensitivity, priority ties, size filters, CLI/environment
precedence, config preservation/creation, stderr output, error recurrence, grouping, headless UI
rendering, and the public Windows snapshot, action, error, and restored-geometry contracts. Windows
tests mutate only fixture-owned windows. The monitor cache remains a private implementation detail;
its snapshot-local ownership and retry boundary are documented rather than instrumented for tests.

The TOML example is parsed and compiled by a core regression test. Core fake-backend tests verify
queue ordering, failure isolation, refresh behavior, name-based identity, grouping, and logging
deduplication. Schema-only tests stay in the core integration suite. Verification uses release-mode
workspace tests and Clippy with warnings denied. Formatting is manual; no formatter is run.
Interactive UI checks remain manual.
