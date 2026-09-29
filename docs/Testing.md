# Testing convention

Tests exercise public behavior rather than internal representation. Keep test bodies,
fixtures, and recording backends under each crate's `tests/` directory. Production
modules do not include private-access test modules or expose APIs solely for assertions.

## Placement and observation

1. Put integration suites in `tests/<subject>.rs` and shared fixtures in
   `tests/support/<name>.rs`, explicitly included by their consumers. Each suite must
   compile once; fixture-only files must not become Cargo test targets.
2. Exercise parsing and verification in engine tests, and intrinsic value queries and serialization
   in core tests. Schema/parser agreement belongs to the engine integration suite.
   Test algorithm results, boundaries, error behavior, and intended use through their
   existing public contracts.
3. Exercise EngineImpl through construction, queueing, updates, snapshots, views, and rule
   lookup. A fake platform can share a recorder with its test through `Rc<RefCell<_>>`;
   the engine still owns the platform. Assert calls and returned values without
   extracting the backend, reading rule storage, comparing pointers, or counting Rc owners.
4. Exercise App through `eframe::App::ui` and `eframe::App::logic`. Headless tests use
   egui pointer events and eframe's existing `Frame::_new_kittest()` constructor.
   Find controls through rendered text, then observe queued engine commands after logic runs.
   No test helper reads App fields or invokes private rendering/scheduling functions.
5. Observe polling through engine updates and real elapsed time with margin beyond the
   polling interval. Slow captures verify completion-based pacing for failures and
   recovery; clicked actions verify immediate dispatch. These tests intentionally
   cover observable scheduling rather than an exact private timestamp boundary.
6. Native tests create hidden or nonactivating offscreen windows, mutate only those
   fixtures, and clean up on the creating thread. Snapshot tests may enumerate the
   desktop but assert only fixture outcomes. Never require a particular user app to
   be open or modify unrelated windows.

## Coverage boundaries

Windows snapshot tests cover empty-title exclusion, title recovery, and retention of
non-shell windows named Program Manager. Public native action tests cover contextual
errors and preserved native causes. Failed-capture/filter combinations and synthetic
Explorer executable identities cannot be injected deterministically through public
snapshots; no private test seam exists to manufacture those states. Backend cache
layout and eviction bookkeeping remain internal mechanisms; generic Cache behavior
is tested separately through lazy insertion and eviction results.

UI fixtures supply explicit group/window action availability and repeat their final prepared
view for action-triggered updates. They implement the core Engine contract without importing
EngineImpl, parsing config, matching rules, or deriving satisfaction. Their recorder distinguishes
queued commands from commands drained by update. UI timing tests use slow engine updates.
Original fixture errors remain available as borrowed chains; diagnostic formatting stays in App.

## Isolation and maintenance

1. Keep process-global logger and environment tests in isolated integration binaries.
   Do not install competing loggers in a shared test process.
2. Share fixture construction while keeping expected results visible in each case.
   Do not add production fields, getters, clock injection, or public visibility solely
   to satisfy a test. Preserve source intent comments near the implementation.
3. Keep all executable test bodies outside production source, including doc-test bodies.
   Follow repository style manually; do not run a formatter during migrations.

## Validation

```nu
cargo test -p turbozone-engine --test engine --release --locked
cargo test -p turbozone-ui --test ui --release --locked
cargo test --workspace --release --locked
cargo clippy --workspace --all-targets --all-features --release --locked -- -D warnings
```

Check discovery after moves: suites must run as integration targets and fixture files
must not appear as independent empty targets. Keep test dependencies scoped to tests.
