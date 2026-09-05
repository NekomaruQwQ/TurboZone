# TurboZone design

## Configuration contract

TurboZone is rule-driven, without configurable groups or rule inheritance.
The Rust config types define the accepted structure. An explicit generator binary produces the
canonical schema referenced by configs for [editor completion and validation](Schema.md).
[config.example.d.ts](config.example.d.ts) illustrates the shared configuration types; [config.example.toml](config.example.toml) is a validated example.

Each `Rule` contains:

1. A required unique `name`.
2. An optional authored `description`; `Rule::display_name()` trims on access and falls back to the name.
3. `program` and `window` filters containing authored `Pattern` values.
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

`turbozone-core::parse_config()` deserializes the entire document before semantic validation.
Invalid TOML, top-level structure, or any individual rule rejects the complete configuration;
partially verified rule sets are never exposed. Successful rules retain declaration order.
An empty document is a valid configuration with no rules.

`parse_config()` returns the same `Config` model used by Serde and schema generation.
Configured sizes are `euclid::default::Size2D<i32>` values shared directly with runtime
geometry. Euclid’s Serde support preserves `[width, height]` pairs, and schema overrides
retain the fixed-array input contract.
`verify_config(&Config)` can also check deserialized or manually constructed values. It logs the
first semantic failure and returns `None`, or `Some(())` after every rule passes. Verification
changes no authored text, resize variants, or rule order. `Engine::new` takes ownership of
verified `Vec<Rule>` values and exposes borrowed rules; direct callers must verify first.
Description trimming is a borrowed query and program lowercase conversion occurs only in matching.

## Startup and diagnostics

1. Select `NekomaruQwQ/TurboZone/config.toml` under the Windows local application-data known
   folder. The executable has no config CLI or environment override. The loader requires an
   absolute file path and rejects empty or relative paths before filesystem access.
2. Leave existing config bytes untouched, including comments, schema directives, BOM, and line
   endings. Create a missing file exclusively with only the canonical remote `#:schema` comment.
   Parent directories must already exist. Concurrent creation never authorizes overwriting the file.
3. Parse and verify transactionally via `turbozone-core`, log a configuration error at its
   failing stage, then launch the UI with source-ordered `Rule` values owned directly by
   `Engine` only after complete validation.

Unreadable config, creation failure, and invalid documents exit nonzero before the UI opens.
Startup never generates schemas, and user-authored config is never rewritten automatically.

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

| Configuration | Primary size | Selector settings | UI |
| --- | --- | --- | --- |
| Omitted or `resize = false` | None | None | No resize controls |
| `resize = true` or `resize = {}` | None | Unbounded limits | Size selector |
| `resize = [1440, 900]` | That default | That default with unbounded limits | Primary button plus selector |
| `resize.exact = [1440, 900]` | That size | None | Exact resize button only |
| `resize = { default = [1440, 900], min = [960, 540], max = [3840, 2400] }` | That default | Those settings | Primary button plus selector |

`ResizeRule::primary_size()` and `ResizeRule::selector()` interpret these modes on demand.
The authored enum remains the source of truth; exact mode cannot coexist with selector mode.

Selector `default`, `min`, and `max` are all optional. Exact and selector fields cannot be
mixed. All sizes are two-element arrays `[width, height]` of physical-pixel integers from `1`
through `16,384`; incomplete pairs, extra components, fractional dimensions, and larger values
are rejected.

Minimum bounds must not exceed maximum bounds on either axis. Resize bounds constrain both
the default target and built-in menu choices, inclusively. A well-formed default outside either
bound logs a warning during verification and is unavailable to both group and per-window primary
buttons. The configuration remains usable, its authored default stays intact, and bounded selector
choices remain available. Runtime queries do not repeat the warning. Invalid dimensions and
inverted bounds still reject the complete configuration. A default or exact target need not appear
in the built-in manifest. A selector with no surviving choices shows an explanatory message.

Position and resize permissions are independent. A matched rule with neither action remains an
intentional actionless group. Native controls require both a successful window-detail query
and a matching rule that enables the action.
Configuration validation prevents pathological dimensions but does not guarantee that every
accepted size can be applied. Native resize operations remain fallible because the compositor
or target application may impose a smaller dynamic limit.

## Patterns and rule selection

A pattern string matches exactly. A partial object supports `starts_with`, `ends_with`, and
`contains`; all nonempty components are ANDed, and at least one must be nonempty. Exact-string
objects, regexes, and globs are not accepted.

Configuration matching compares program paths and names case-insensitively through
`Pattern::matches_ignore_case()`. It uses the existing Unicode-aware SmolStr lowercase conversion
on the candidate and needed literals at match time, preserving authored patterns. Titles use
case-sensitive `Pattern::matches()`. Both methods evaluate exact or ANDed partial patterns
directly, without predicate vectors or function pointers. Entirely empty partials are rejected
by validation and fail closed if constructed manually. Absent filters remain `None` and need no
matching conversion. Matching normalization is independent of platform-backend cache identity.

`Engine` directly owns one source-order `Vec<Rule>` and exposes it as a borrowed slice.
For each `WindowInfo` with complete details:

1. Scan all rules in source order.
2. AND all configured filters.
3. Replace the winner only when a matching rule has strictly greater priority.
4. Equal priority therefore favors the first rule.

`matching_rule_name(rules, window)` returns the winning name and `Engine::rule(name)` resolves
it. Name-based lookup preserves source-order UI groups without allowing array positions to escape
as identity. No additional priority index is needed for the expected
rule/window counts.

## Window geometry and snapshots

The core crate owns `WindowInfo<H>`, `WindowDetail`, `ProgramInfo`, `WindowState`, and the
generic `Backend` contract. The Windows crate owns the concrete `Handle<HWND>` and returns
`WindowInfo<Handle<HWND>>`. A `WindowDetail` holds its immutable `ProgramInfo` through `Rc`,
allowing a backend to share one program snapshot between multiple windows. Owned UTF-8 text
crossing TurboZone API boundaries uses `SmolStr`, including serialized config values,
diagnostics, startup file contents, runtime names, titles, paths, and matcher literals.
Standard-library and serializer helpers may construct temporary buffers before conversion;
persisted bytes and external string formats remain unchanged.

Every snapshot retains its handle, title, and visual state. Its `detail` is either:

1. `Ok(WindowDetail)`: monitor work area, controllable client rectangle, process ID, normalized
   program path, matching filename, and human-facing program description are all available.
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

Each `Backend::snapshot()` call owns call-local monitor and process-path caches. Monitor results are
shared by all window-detail queries for that monitor, including failures. Process-path results are
shared by process ID so multiple windows from one process issue only one path query during the
snapshot. Both caches are discarded when the call returns: monitor work areas may change, monitors
may disappear, and process IDs may be reused.

The Windows `Backend` separately caches complete program results by the exact native `OsString`
path. It does not case-fold or normalize separators for cache identity. Successful entries share an
immutable `Rc<ProgramInfo>`; deterministic path-conversion failures can occupy the same cache.
Missing, empty, or unreadable version metadata produces a successful program detail whose display
description falls back to the program filename. Program-cache retention is an internal backend
policy rather than part of the core snapshot contract. Existing snapshots keep their program
details alive independently through `Rc`. Native actions query current geometry separately.

## Size filters

`window.min` and `window.max` filter rule eligibility using `content_rect.size`.
They are independent of `resize.min` and `resize.max`, which constrain resize defaults and selector choices.

Bounds are inclusive on both axes, limited to `1` through `16,384` when present, and otherwise
unrestricted. Normal windows use live client size; minimized/maximized windows use restored
client size. Any detail-query failure excludes the whole snapshot from matching, even for
unfiltered rules.

Matching is reevaluated on every snapshot, so moving, resizing, or query recovery may change a
window's winning rule.

## Classification and UI

Snapshots are logged before classification, then only complete matches are retained:

```text
WindowInfo
    Err(error) -> log first/changed failure, discard snapshot
    Ok(detail), no winning rule -> discard normally
    Ok(detail), winning rule -> (rule, program path) group
```

`group_windows()` moves matched snapshots into `Vec<Group<H>>` without cloning the window
snapshots. Groups are ordered by valid rule source order, then lowercase program path. The
lowercase path remains an internal grouping and ordering key; each group exposes the first matched
window's shared `Rc<ProgramInfo>`, preserving display casing without an arbitrary later lookup.

Windows from the same program matching the same rule share a group. Different paths or winning
rules form separate groups. Actionless matched rules still appear. The flat UI keeps group and
window actions, program paths and descriptions, titles, visual states, and current-size markers.
It deliberately omits process IDs and read-only badges. Program descriptions are display-only;
configuration continues to match executable filenames.

A geometry failure removes the whole snapshot from matching until a later snapshot succeeds.
Old details are never substituted. Unmatched windows are normal, with no warning or diagnostic
bucket. An empty matched list shows the ordinary empty state. There is no diagnostics page,
page navigation, error card, diagnostic count, or configuration-path heading.

## Native paths and actions

Windows is assumed to return paths with backslash separators. The native path must be representable
as Rust UTF-8 text; otherwise program-detail capture fails and the complete window detail is
unavailable for that snapshot. After conversion, the adapter replaces backslashes with forward
slashes without lexical normalization or filesystem canonicalization. Display casing is preserved.
Configured patterns are not path-normalized, and backslashes in them are rejected.

Centering aligns the client-area center with the monitor work-area center. Resizing preserves
the integer client-area center, including odd-sized targets. Actions preserve activation, z-order,
and normal/maximized/minimized state; restored actions update placement rather than restoring
the window. Oversized targets that cannot fit native dimensions return errors.

Individual actions capture one handle from the rendered snapshot. Group controls append one
action per eligible handle. Core's non-exhaustive `WindowAction` currently contains
`Resize(size)` and `Center`. The engine queues `(handle, action)` pairs and drains them in order
by calling `Backend::perform(handle, action)`, then refreshes through `Backend::snapshot()`. The backend owns variant
dispatch and deliberately panics on a future unsupported variant rather than silently doing nothing.

The eframe app records the last completed logic tick and calls the engine immediately at startup,
when an action is pending, or at the presentation-owned 1 Hz cadence. Painting remains independent
at 30 Hz. A snapshot is not an atomic OS
transaction: windows can disappear or change between queries and actions, so native actions
remain fallible.

If top-level enumeration fails, stale matched groups are cleared and the enumeration error
is logged. Per-window detail failures do not invalidate other windows. User-requested action
failures are always logged and do not abort actions for the remaining targets.

## Source layout

1. `turbozone-core/src/`: serialized config and schema types, validation, runtime matching,
   backend contract, action queue, snapshot lifecycle, stable groups, log deduplication, and window
   models. It performs no filesystem I/O and does not install a logger.
2. `turbozone-ui/src/`: generic `App<B>`, app-owned scheduling cadence, flat egui presentation,
   and startup config filesystem operations. It has no dependency on the Windows crate.
3. `turbozone-windows/src/`: native handles, snapshot adaptation and caching, geometry queries, and
   the concrete Windows `Backend`. It has no dependency on the UI or executable crate.
4. `turbozone/src/`: the Windows composition root, config-path selection, logger initialization, UI/backend assembly,
   and the `turbozone.exe` entry point.

## Verification

Integration tests cover schema/serialization, transactional config rejection, all resize modes,
selector bounds, exact and partial matching, case sensitivity, priority ties, size filters,
absolute-path validation, config preservation/creation, stderr output, error recurrence, grouping,
headless UI rendering and clicks, and the public Windows snapshot, action, error, and restored-geometry
contracts. Startup loader tests live with the `turbozone` composition root and use fixture-owned paths;
diagnostic subprocesses execute only a test probe. The production executable's known-folder
selection and GUI launch are not automated. Windows tests mutate
only fixture-owned windows. The monitor and process-path caches remain private implementation details; their
snapshot-local ownership is documented rather than instrumented directly. Public snapshot tests
cover the filename fallback when version metadata is unavailable. Core integration tests cover the
generic cache's lazy insertion, key identity, hit refresh, and eviction boundary. Windows-specific
program-cache identity and its 600-tick retention policy do not yet have direct instrumentation.

The schema generator is an explicit core binary, and the TOML example is parsed
and verified by a core regression test. Core fake-backend tests verify
queue ordering, failure isolation, refresh behavior, name-based identity, grouping, and logging
deduplication. Schema-only tests stay in the core integration suite. Verification uses release-mode
workspace tests and Clippy with warnings denied. Formatting is manual; no formatter is run.
Interactive UI checks remain manual.
