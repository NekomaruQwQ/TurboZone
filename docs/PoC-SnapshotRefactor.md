# Proposal of Change: complete snapshot refactor

Status: approved on 2026-09-07, including raw snapshots and on-demand EngineView refinement. Implemented and validated.

This proposal continues unnamed change `yptnlwmo` (`bf673e4a`), compared with
`ktlklstn` (`b1804cec`). The working copy was empty when investigation began.
The user explicitly requested both enumeration and per-window failures in the UI.
The initial investigation changed only planning documents. The subsequent approval covers implementation of the three PoCs with the refinements below.

## Recommended architecture

Keep platform capture, engine classification, and UI presentation separate. A backend
returns a Snapshot containing successful windows and handle-keyed failures; the engine owns rules,
the raw snapshot, and comparison with the preceding snapshot. Grouping is an uncached borrowed query. The UI renders that
state and decides when to request another update.

```rust
pub struct Snapshot<H> {
    pub windows: Vec<WindowInfo<H>>,
    pub errors: HashMap<H, anyhow::Error>,
}

pub struct EngineView<'a, H> {
    pub groups: Vec<Group<'a, H>>,
    pub errors: &'a HashMap<H, anyhow::Error>,
}

pub struct Group<'a, H> {
    pub rule_name: &'a SmolStr,
    pub program: &'a ProgramInfo,
    pub windows: Vec<&'a WindowInfo<H>>,
}

pub trait Backend {
    type Handle: fmt::Debug + fmt::Display + Copy + Eq + Hash + 'static;

    fn snapshot(&mut self) -> anyhow::Result<Snapshot<Self::Handle>>;
    fn perform(&mut self, target: Self::Handle, action: WindowAction)
     -> anyhow::Result<()>;
}
```

`Snapshot` is parameterized by the handle, matching `WindowInfo` and `Group`.
It need not depend on a backend implementation. This also removes the draft's
need to require `Backend: Sized` merely to express its snapshot return type.
The per-window result remains a backend capture boundary. The public Backend boundary returns partitioned windows and errors.

`Engine::view(&self) -> Result<EngineView<'_, B::Handle>, &anyhow::Error>` builds an
uncached view containing `groups: Vec<Group<'_, H>>` and a borrowed error map. Each
Group borrows its rule name, representative ProgramInfo, and windows. No snapshot
window is cloned or consumed by grouping. Successful unmatched windows remain in
the raw snapshot. Calling view performs no native queries or logging.

```text
Before
Windows -> WindowInfo { handle, title, state, detail: Result<WindowDetail> }
        -> separate SnapshotLogging history -> matching/grouping -> successful UI

After
Windows -> Result<Snapshot { windows, errors }, Error>
        -> Engine: compare previous state, log, store + completion timestamp
        -> Engine::view(): borrow snapshot, match/group successful windows
        -> App: successful controls + snapshot diagnostics
```

For example, a resize click queues `(handle, Resize(size))`. The next update attempts
every queued action in order, logs each action failure using the handle's Display,
and captures a new backend snapshot. A window whose geometry query fails contributes
only `(handle, error)`; another successful window remains available for subsequent matching and grouping. The engine logs new or changed failures and replaces its complete state.
The UI refreshes its diagnostic text from that state and requests EngineView when rendering. An enumeration failure instead
replaces the entire state with an outer error, removing all previous action targets.

## Baseline established during review

1. `WindowInfo` is already flattened. `WindowDetail` and `logging.rs` are deleted.
   Matching, grouping, the Windows adapter, UI, and fixtures still use the old fields.
2. The draft backend signature returns `Snapshot<Self>`, which already contains groups.
   The actual adapter still returns a vector and has no rules with which to group it.
3. `Engine::update()` still references deleted `logging` and nonexistent
   `snapshot_groups` fields. Its new snapshot and timestamp are never updated.
4. `Engine::rules()`, `groups()`, and `into_backend()` have been removed. Tests and UI
   still call them. The included `EngineTestExt` is empty and compiled unconditionally.
5. Core now exports `config_parser` as a module without root function re-exports.
   Runtime loading and tests still import the old root paths.
6. The non-CatIgnored UI source now polls at 3 Hz, while old documentation says 1 Hz.
   Preserve the source setting. Painting remains 30 Hz.
7. Windows handle Display already reads the current title and falls back to a hex
   identity. Its change in the unnamed commit was only a final newline.
8. Tests already reside outside `src/`; the missing convention is how private access
   works without adding test methods to the production API.

`cargo check --workspace --all-targets --release --locked --offline` was run. It stops
in core with seven errors (removed fields and mismatched snapshot types) and an unused
`Duration` import warning. Downstream failures were identified by inspection, not by
a successful downstream build. This was the pre-implementation baseline; completed validation is recorded below.

## PoC 1: backend capture and engine snapshot lifecycle

### Contracts submitted for approval

1. Preserve every flattened field in `WindowInfo`, including title, visual state,
   geometry, PID, and shared `Rc<ProgramInfo>`. Success means all required fields were
   captured. Title and program description retain their existing optional/fallback
   behavior; flattening does not turn them into newly mandatory queries.
2. `Backend::snapshot()` has the raw result signature above. An outer error means
   enumeration failed and supplies no usable partial enumeration. Each included
   handle occurs once, either as a success or a failure. Duplicate or contradictory
   entries violate the backend contract; this work adds no conflict-resolution policy.
3. The backend continues to decide which native windows are relevant. The engine
   alone applies configured rules. Failed relevant windows are retained even when
   no rule could be evaluated or there are no configured rules. Successful unmatched
   windows remain in the raw snapshot and are omitted only from EngineView groups, not diagnosed.
4. The engine stores `anyhow::Result<Snapshot<B::Handle>>` and exposes it through
   `snapshot(&self) -> &anyhow::Result<Snapshot<B::Handle>>`. Do not restore `groups()`
   as a fallback accessor that hides enumeration failure.
5. `update()` drains actions, captures, compares/logs, and replaces state. `view()` classifies only when requested.
   Action failures never abort later actions or refresh and do not become snapshot
   errors. They remain individual error logs even when identical actions fail twice.
6. Retain `snapshot_timestamp: Instant`; expose
   `snapshot_timestamp(&self) -> Instant`. Set it when every update completes, including
   enumeration failure. Before the first update it is the construction time, paired
   with an empty successful snapshot. `App::new()` still updates immediately.
   This is the latest attempt's completion time, not when a persistent error began.
7. The App uses this timestamp for its existing cadence, replacing its duplicate
   `last_update`. Scheduling stays in the App: immediate startup, pending actions,
   or 3 Hz periodic refresh. Core does not initiate timers or retries.

### Classification and native capture invariants

Only successful `WindowInfo` values enter `matching_rule_name`, `matches_rule`, and
`group_windows`. Their successful-input algorithms remain available. `group_windows` now takes a borrowed window slice and returns borrowed groups. Remove detail guards and unwraps; their input type now expresses
the completeness precondition. Keep source-order priority ties, inclusive size bounds,
case-insensitive program matching/grouping, case-sensitive titles, first-window
representative program casing, and window enumeration order within groups.
Do not create empty groups. The backend owns windows and errors; groups borrow their windows. Do not reconstruct or clone errors.
Retain native causes so callers can inspect/downcast them.

The Windows adapter keeps capture temporaries local. It must retain enough local
title information to apply existing ignore rules even if required capture fails:

| Candidate | Backend output |
| --- | --- |
| Not a visible, unowned, uncloaked app window | Omitted, as today |
| Empty/unreadable title, including a failed required query | Omitted, as today |
| `Program Manager` with successful `explorer.exe` identification | Omitted |
| `Program Manager` whose required capture failed | Handle-keyed failure; do not guess its executable |
| Other relevant window with a required query failure | Handle-keyed failure |
| Other complete relevant window | Flattened success |

Preserve existing required-query order, first-error behavior, and cache touches.
An early title filter could avoid work but would change which program entries are
refreshed; do not introduce that optimization incidentally. Snapshot-local monitor
and process caches, exact native program-path keys, UTF-8/path conversion, filename
description fallback, and persistent `Rc` sharing remain unchanged.

### Adjacent-snapshot log deduplication

The previous snapshot is the only history. Compare complete alternate-Display error
chains (`{error:#}`), not pointer identity, error Debug, handle descriptions, or only
the outer message. The native error object remains in the snapshot; rendered chains
are temporary comparison values. Error types with identical rendered chains remain
indistinguishable for deduplication, matching the previous message-based policy.

| Previous state | Next state | Logging |
| --- | --- | --- |
| Successful enumeration, same handle and same error chain | Same failure | Silent |
| Successful enumeration, missing handle or changed chain | Window failure | Warn once |
| Window failure | Handle succeeds, disappears, or is filtered out | Forget it; no recovery log |
| Recovered/absent handle | Identical failure recurs | Warn again |
| Anything except the same outer chain | Enumeration failure | Error once |
| Enumeration failure | Same enumeration chain | Silent |
| Enumeration failure | Different cause under the same outer context | Error again |
| Enumeration failure | Successful enumeration containing window failures | Warn for those failures |
| Enumeration failure | Successful enumeration without failures | Clear the error; no recovery log |

The penultimate row deliberately changes previous behavior. The old logger retained
window-error history across enumeration failures. With a single latest `Result`, an
outage breaks that history, so the same window failure is reported on resumption.
Preserving outage-spanning deduplication would require separate historical state;
that alternative is not recommended for this design. Repeated alternating global
and per-window failures can therefore produce one log on each transition.

Per-window warning order follows the error map and is unspecified; UI diagnostic rows are sorted independently. Compare before dropping the previous state. Logging
filters must not change what errors are stored or displayed.

### Handle descriptions

Core diagnostics obtain window identity only through `Handle::Display`. Successful
window titles and group program metadata remain captured presentation data; they are
not a fallback identity source for a failed window or failed action.

For Windows, retain the existing hex identity plus optional current title. Escape the
quoted title so quotes, line breaks, and control characters do not corrupt one log
record. Title-query failure falls back to hex and is not a formatting failure;
only a formatter write failure returns `fmt::Error`. Do not add process/version queries
or backend-global caches to Display. Eq/Hash continue to depend only on native identity.

The description is best effort at formatting time. A destroyed window can lose its
title; a reused handle may describe a different window. The previous action logger's
cached executable path is no longer guaranteed. Neither error keys nor deduplication
depend on the description. Existing backend action context and native causes remain
intact; backend and engine context can both mention the handle.

GetWindowText has different same-process and cross-process behavior and can block for
an unresponsive same-process window. This is an existing native limitation, not a
new guarantee of a nonblocking Display implementation.
[Microsoft's GetWindowTextW contract](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw).

### Implementation latitude and alternatives

Partitioning belongs to the backend; grouping is a separate, uncached borrowed query.
Internal helpers may vary provided ownership, ordering, and error isolation remain as specified. Error comparison
can be a private helper. No new dependency, shared error wrapper, logger component,
backend grouping responsibility, or persistent dedup cache is needed.

## PoC 2: snapshot failures in the existing UI

### State and presentation contract

| Engine snapshot | Presentation |
| --- | --- |
| Outer enumeration error | `Couldn't read windows`, wrapped error chain, `Retrying automatically.`; previous groups and window errors are absent |
| Groups plus window errors | Existing groups and controls, followed by `Windows with errors (N)` and failed-window rows |
| No groups, some window errors | Failed-window section; omit the misleading ordinary empty message |
| No groups or errors | Existing `- nothing here -` state |
| Groups, no errors | Existing group view |

Failures remain visible on every frame even when the corresponding repeated log is
suppressed. Every failed row identifies its window using Display and exposes its full
contextual chain. Failed rows have no center/resize controls. Successful groups remain
usable when other windows fail. Recovery replaces the diagnostic on the next update.
Action failures alone do not create new persistent UI state in this refactor.

Keep the fixed 450-by-720 viewport and existing dark egui hierarchy. Use the existing
text roles, semantic warning/error colors, and a separator before failed rows. Titles
and details wrap; do not make truncation the only way to view the cause. Use selectable
error text where practical. No new navigation, modal, retry button, animation, font,
palette, or count outside the section is required. The distinctive behavior is that
usable groups remain directly actionable while failed windows are explained below.

Illustrative layout:

```text
Application description                  rule.name
C:/Apps/Application.exe
    Existing successful-window controls

-------------------------------------------------
Windows with errors (2)
0x1234 ("Editor")
    Failed to get program path: Access is denied.
0x5678 ("Preview")
    Failed to get content rect: Window disappeared.
```

### Presentation ownership

The App captures failed-handle descriptions and rendered error chains once after
each engine update, including initialization. Paints reuse those strings. The engine
keeps the authoritative WindowInfo and Error objects; the App owns only derived presentation text.

```rust
// Private to the UI; the concrete builder is a local implementation choice.
struct WindowDiagnostic<H> {
    handle: H,
    description: SmolStr,
    message: SmolStr,
}

// App fields, alongside engine:
// window_diagnostics: Vec<WindowDiagnostic<B::Handle>>
```

Rebuild the vector after every update and clear it on outer failure. Sort by captured
description, whose Windows representation starts with stable native identity. Equal
descriptions need no additional order contract. Use `(diagnostic-kind, handle)` as
egui identity, never a vector index or the mutable title. The outer error can render
directly from `Engine::snapshot()`; it requires no handle lookup. Do not use timestamp
equality as a change-generation counter; refresh the presentation explicitly when
the App calls `Engine::update()`.

Make the rendering entry point a private App method over an immutable App borrow.
The eframe callback collects emitted actions and queues them afterward, preserving
the current no-native-effects-during-paint boundary. `AppTestExt` will invoke this
same entry point, including its derived diagnostic state. Specific spacing, wrapping,
and private helper structure are delegated within this presentation contract.

Rendering Display directly on every paint is simpler, but would turn a persistent
failed window into approximately 30 native title reads per second. A presentation
cache tied to actual updates avoids that cost without moving descriptions into core.

## PoC 3: test access and migration

Adopt the [testing convention](Testing.md): public API tests remain ordinary
integration tests; tests requiring private seams are physically under `tests/` but
compiled as child unit-test modules using `#[cfg(test)]` and `#[path]`. The extension
trait lives below the module it accesses and its source file lives in `tests/include/`.

The concrete Engine seam exposes borrowed rules and consuming backend extraction.
It does not restore `groups()`: assertions must observe the actual snapshot Result.
The App seam invokes the private renderer and update/cadence helper. A Windows backend
seam may invoke single-window capture with a test-supplied handle for deterministic
failure tests, reusing production capture and cache context rather than duplicating it.
These traits compile only into their crate's unit-test target. Shared fixtures compile only in the test targets that include them.

Do not add a feature or public test-support module merely to retain the integration
test harness for private tests. Cargo integration tests cannot access a dependency's
`cfg(test)` helpers; move those tests into the included unit-test suite instead.
[Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html).

Existing public matching/grouping functions, Cache, and native geometry exports remain
available in this scope. This is not a general audit or pruning of crate APIs.

### Complete affected test families

| Current location | Migration and retained evidence |
| --- | --- |
| `core/tests/engine.rs` | Move to `core/tests/unit/engine.rs`, included through `engine_ext.rs`; retain queue ordering, drain, failure isolation, authored rules, and fresh-state checks |
| `core/tests/logging.rs` | Keep a separate integration binary, replace SnapshotLogging calls with scripted real Engine updates; test the full transition table and action logs |
| `core/tests/data.rs` | Flatten successful fixtures and preserve ordering/casing/Rc grouping checks; relocate failed-detail cases to Engine raw-state/recovery tests |
| `core/tests/verification.rs` | Keep config and successful matching tests; flatten fixtures; move the impossible incomplete-window matcher test to an Engine failure-exclusion test |
| `core/tests/smol_strings.rs` | Flatten its window fixture; preserve Unicode, long text, and schema checks |
| `core/tests/config_queries.rs`, `config_sizes.rs`, `config_logging.rs`, `patterns.rs`, `resize_warnings.rs`, `schema.rs` | Update parser/validator imports to `turbozone_core::config_parser`; retain existing semantics and separate logging processes |
| `core/tests/cache.rs` | No semantic migration; retain and run existing public cache tests |
| `ui/tests/ui.rs` | Move to `ui/tests/unit/ui.rs`, included through new `tests/include/app_ext.rs`; render the real App via AppTestExt |
| `ui/tests/support/window.rs` | Supply partitioned snapshots and scripted outer errors; the UI unit suite also supplies a handle with distinguishable Display/Debug; preserve the panic on render-time native actions |
| `windows/tests/window_backend.rs` | Check the fixture handle in the error map, then find its successful window; never discard its error with filter_map; retain geometry, fallback, action, and native-cause tests |
| New Windows private capture/filter tests | Include from `tests/` under backend's test module; validate failure handle/cause retention and title/Program Manager filtering without depending on a user's inaccessible windows |
| `windows/tests/window_geometry.rs`, `tests/support/test_window.rs` | Retain existing separate-file geometry tests and fixture ownership; add title/Display cases using fixture-owned windows |
| `turbozone/tests/startup.rs` and support | Retain config byte preservation and subprocess diagnostics; run after the config loader import migration |

Here `core`, `ui`, and `windows` abbreviate their existing `crates/turbozone-*` folders.

### Additional regression requirements

1. Mixed results retain all failures and classify only successes, including empty rules,
   unfiltered rules, success-to-failure, failure-to-success, absent/reappearing handles,
   and recovery with changed program/geometry. Downcast a stored error to verify it was
   moved intact. No stale group or per-window error survives an outer failure.
2. Logging covers equal, changed, recovered, missing, and recurrent errors; changed
   nested enumeration causes; outage/resumption; and identical repeated action failures.
   A fake handle whose Debug differs from Display proves identity uses Display. Changing
   its description alone must not defeat per-window log deduplication.
3. `WindowInfo::is_centered()` gets separate-file examples for centered/offset geometry
   and integer-center behavior; preserve existing matcher priority, bounds, and Unicode tests.
4. UI tests cover every row of the presentation table, full failure identity/chain,
   no actions on failures, successful controls beside errors, recovery, mixed groups,
   stable row IDs/order, long/Unicode/error text, and bounded layout. Count fake-handle
   Display calls to prove repeated paints do not query identity again.
5. Test cadence with a supplied Instant through a private helper exposed by AppTestExt:
   before the 3 Hz deadline, at the deadline, and with pending work. Do not sleep or
   inject a clock abstraction into the backend. Both success and failure update the
   completion timestamp. Tests must not assume distinct calls yield distinct Instants.
6. Windows Display covers a titled fixture, empty title, invalid handle, and escaped
   quotes/newlines/Unicode. Tests create/mutate only their own hidden or nonactivating
   offscreen windows. Do not rely on deterministic handle reuse or desktop contents.
7. Keep logger assertions isolated: one serialized transition test or a scoped capture
   mechanism per logging binary. Moving private tests into the library harness must
   not create competing global logger initialization or cross-test message contamination.

## Other behavior changes and preserved constraints

1. The current 3 Hz cadence is already a user source change and stays. At steady polling,
   600 backend sequence steps represent roughly 200 seconds instead of 600 seconds at
   1 Hz. Actions can advance them faster. Existing code increments the sequence before
   enumeration, including failed attempts, and skips eviction on an enumeration error.
   Do not convert this policy to wall-clock retention or silently change its threshold.
2. New UI diagnostics reveal failures even with logging disabled, and reveal failures
   among relevant windows that could not be matched. This is the user's requested UI
   expansion. Program paths/title text may appear in existing error chains or Display.
3. UI text can differ from log identity if the native title changes between the two
   formatting calls. Each is best effort; neither promises a historical title. Captured
   successful titles remain the rule-matching and successful-row source of truth.
4. Error HashMaps retain original errors until the next snapshot; previous storage retained
   rendered strings. Comparison now formats prior/current chains during updates. Memory
   retains all current successful windows and errors, plus transient previous/next state,
   borrowed view membership lists, and cached UI diagnostic text.
5. Flattening removes invalid-input branches from grouping and view rendering. Empty
   groups remain forbidden so `.all()` cannot accidentally enable a vacuous group action.
   The draft's explicit primary-size presence check remains intact.
6. The core parser module move requires both test imports and
   `turbozone-ui/src/config.rs` to change. Preserve the authored module path instead of
   restoring aliases. Fix related rustdoc links; no configuration format changes.
7. Native query errors remain all-or-nothing for required fields. Geometry algorithms,
   state preservation, foreground behavior, cache keys, and normalization are unchanged.
   No dependency/toolchain upgrade, formatter, or real user-window mutation is included.

## Approved implementation order and validation

1. Land PoC 1 and its directly required migrations as one coherent unit: core model,
   Windows adapter, engine transitions, parser call sites, and minimum UI consumption of
   the Result. Establish test-only Engine access when migrating its tests. This stage
   must compile and its focused core/Windows tests must pass; it is not the completed
   feature until the requested failure UI lands.
2. Land PoC 2 with AppTestExt and the UI suite migration. Complete diagnostic derivation,
   display states, cadence timestamp use, and pointer-event regressions. At this stage
   the application presents all requested snapshot outcomes.
3. Finish PoC 3's remaining fixture/import/documentation migration and verify discovery.
   Mark Testing.md adopted, reconcile this proposal, and update docs/README.md's old
   WindowDetail, logging ownership, no-diagnostics, 1 Hz, and test-layout claims. Record
   test moves and retained assertions so migration does not silently delete coverage.

The PoCs are separate approval units, but related tests move with the code they verify.
Use separate commits for these logical implementation units. Preserve the unnamed
parent and unrelated config formatting changes.

Planned commands (run in the repository root, without a formatter):

```nu
cargo test -p turbozone-core --release --locked
cargo test -p turbozone-ui --release --locked
cargo test -p turbozone-windows --release --locked
cargo test --workspace --release --locked -- --list
cargo test --workspace --release --locked
cargo check --workspace --all-targets --release --locked
cargo clippy --workspace --all-targets --all-features --release --locked -- -D warnings
```

Run focused suites as each unit becomes buildable, then the workspace suite and Clippy.
Use discovery output to check included unit tests as well as integrations. Search for
remaining `WindowDetail`, `.detail`, `SnapshotLogging`, old Engine accessors, and old
parser paths, excluding historical proposal text. Verify production modules contain
only test-module declarations, not test bodies or extension implementations.
Default production builds must not expose the test traits or compile test fixtures.
Update CatIgnore for new/broadly changed implementation paths under its normal policy.
The existing user-authored `logging.rs` exact rule is not an agent-managed stale entry;
preserve it unless the user requests its removal.

Headless egui tests establish rendered content and interactions, not subjective visual
quality. An optional user desktop check can assess readability of long error chains in
the fixed viewport. Native desktop automation is not part of validation.

## Approved scope

1. PoC 1: partitioned raw backend snapshots, uncached borrowed EngineView grouping, single-state
   error ownership, adjacent-snapshot deduplication including its outage behavior,
   best-effort Display identity, and completion-time semantics.
2. PoC 2: enumeration and per-window failure UI as specified, presentation-owned cached
   diagnostic text, private App rendering, and existing 3 Hz scheduling from engine time.
3. PoC 3: the external-file unit-test/extension-trait convention, listed test migrations,
   parser-path propagation, source-intent documentation, and validation plan.
4. Local implementation latitude: internal helpers, backend partition helper structure, narrow
   test fixture construction, and UI spacing/wrapping within these contracts. No new
   dependencies or additional public test-support surface.

The user approved all three PoCs, including the subsequent raw Snapshot and EngineView
refinement. Grouping is an uncached borrowed query rather than work performed during update.
No additional architecture changes or dependencies were introduced during implementation.

## As-built reconciliation

1. `47b3063a` implements raw backend snapshots, original-error ownership, full-chain
   adjacent-snapshot deduplication, borrowed EngineView queries, Windows partitioning,
   escaped handle descriptions, parser-path propagation, and directly affected tests.
2. `bef84c9f` implements the failure UI, update-owned diagnostic text, engine-completion
   scheduling, and the App test extension with headless interaction coverage.
3. The final documentation unit adopts Testing.md and reconciles the design guide with
   raw snapshots, on-demand grouping, diagnostics, 3 Hz polling, and test placement.

The implemented public boundary is `Backend::snapshot() -> Result<Snapshot<H>>` and
`Engine::view() -> Result<EngineView<'_, H>, &anyhow::Error>`. Each view owns only its
membership vectors; all source windows, rule names, program records, and errors are
borrowed. The private Windows WindowSnapshot alias retains `(Handle, Error)` at the
single-window capture boundary. There is no public per-window alias or logger object.

All approved runtime units are implemented. The EngineView name and ownership were
approved in the follow-up discussion; there were no unapproved architectural deviations.
Local mechanisms remain isolated: required native capture/cache reuse, native inclusion
filtering, rule matching/grouping, full-chain comparison, and diagnostic text derivation.
Their comments record their responsibilities and invariants without embedding test code.

Completed checks:

1. `cargo test --workspace --release --locked --offline -- --list`: included private
   suites are discovered once (7 core, 13 UI, 3 Windows), alongside public integrations.
2. `cargo test --workspace --release --locked --offline`: 122 tests passed, none failed
   or ignored, including startup subprocesses, native fixture windows, and headless UI.
3. `cargo clippy --workspace --all-targets --all-features --release --locked --offline -- -D warnings`:
   passed with no warnings.
4. Focused core, UI, Windows, logging, and handle-formatting tests passed during their
   implementation stages. No formatter or unrelated dependency update was run.
5. `cargo check --workspace --all-targets --release --locked --offline`: passed.
   Source audit confirms three cfg(test)-gated extension modules and no test bodies
   under src. Removed APIs have no remaining Rust references. Documentation links,
   code fences, and effective CatIgnore matching were checked.

Migration retains queue ordering, grouping/casing, rule priority, size bounds, Unicode,
geometry/native causes, configuration, startup, and resize controls. Previously invalid
WindowDetail inputs moved to the Engine failure boundary. New tests cover borrowed identity,
unmatched-window retention, snapshot transitions, filtered log state, error rows and recovery,
long-text wrapping, native Display fallback/escaping, and failure retry cadence.

Stable diagnostic widget identity was checked directly in the source: the push_id key is
`("window-error", handle)`, independent of description and row index. Sorting and display
reuse are behavior-tested. Desktop visual inspection was not performed; headless tests
verify content, interaction, and width/wrapping in the fixed viewport. No performance
benchmark was added: uncached grouping runs at paint cadence and remains intentionally
simple for current desktop-scale inputs.
