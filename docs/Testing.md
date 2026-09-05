# Proposed testing convention

Status: proposed with [the snapshot refactor](PoC-SnapshotRefactor.md); not yet adopted.

Keep every test body, fixture, test helper, and extension-trait implementation in the
crate's `tests/` directory. Source files may declare externally located test modules.
Physical separation does not require every test to be a Cargo integration test.

## Placement and visibility

| Purpose | File location | Compilation and access |
| --- | --- | --- |
| Public contract tests | `tests/<subject>.rs` | Ordinary integration test; uses the crate's public API |
| Private behavior tests | `tests/unit/<subject>.rs` | Included unit-test child module; uses a narrow test extension trait |
| Private access seam | `tests/include/<module>_ext.rs` | Included under the owning source module, only with `cfg(test)` |
| Shared fixtures | `tests/support/<name>.rs` | Explicitly included by test modules; never production code |

A module's logical parent controls private access independently of its file location.
`#[path]` is relative to the source file declaring that module when the declaration
is outside an inline module body. Put include declarations directly at module scope
to keep those paths straightforward.
[Rust module paths](https://doc.rust-lang.org/reference/items/modules.html).

Keep field access and private-operation adaptation inside the extension file. Test
bodies should use the seam rather than repeatedly depending on the owner's layout.
This is a project convention, not a new Rust privacy barrier: descendants can otherwise
access ancestor-private items. Prefer borrowed views or consuming extraction; expose
mutation only when a concrete test needs to establish an otherwise unreachable state.

## Engine example

At the end of `src/engine.rs`, the only test implementation link is:

```rust
#[cfg(test)]
#[path = "../tests/include/engine_ext.rs"]
mod test;
```

The file `tests/include/engine_ext.rs` owns the seam and includes its callers:

```rust
use super::{Backend, Engine, Rule};

trait EngineTestExt<B: Backend> {
    fn rules(&self) -> &[Rule];
    fn into_backend(self) -> B;
}

impl<B: Backend> EngineTestExt<B> for Engine<B> {
    fn rules(&self) -> &[Rule] { &self.rules }
    fn into_backend(self) -> B { self.backend }
}

#[path = "../unit/engine.rs"]
mod cases;
```

The included `tests/unit/engine.rs` imports `super::EngineTestExt` and production
types through `crate::...`. It declares its tests normally with `#[test]`. Shared
fixtures can be included with `#[path = "../support/<name>.rs"]` from that file.
Keep explicit imports in fixtures so they work under their declared parent; avoid
recompiling `src/lib.rs` through a test-side path include.

The trait needs no `pub` when only its child cases use it. If another in-crate test
module needs it, use the narrowest `pub(super)` or `pub(crate)` route under `cfg(test)`.
Do not make production fields or methods public just to get test access. Existing
public API decisions remain explicit architectural decisions, not an automatic result
of this layout convention.

Use an owner-specific `<Owner>TestExt` name, for example `AppTestExt<B>` for App's
private renderer. A seam forwards to real code; it must not reimplement the behavior
under assertion or replace the production algorithm with a test version. Add accessors
only when an actual test calls them.

## Why included unit tests

Integration tests compile against a library's ordinary public interface. Marking an
extension module `cfg(test)` does not make it available to those integration crates.
Keep private-seam callers in the included unit suite. This provides default Cargo
test discovery without an always-public test module or a special test-support feature.
[Rust test organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html).

Do not place included suites or fixture-only files directly at `tests/*.rs`, where
Cargo would also discover them as independent integration targets. A suite must compile
exactly once. Public API tests should remain independent integration targets when
they require no private seam; not every test needs an extension trait.

## Test quality and isolation

1. Test algorithms' normal results, boundaries, failure behavior, and intended use.
   Keep behavioral assertions when moving a test; replace an impossible old input at
   the new boundary that now owns that failure rather than simply deleting the case.
2. Exercise public behavior where it provides sufficient evidence. Do not add hooks
   for fields that can already be observed through a supported Result or query.
3. Share fixture construction, but keep expected outcomes and the behavior under test
   visible in each test. Avoid general fixture frameworks for a small set of examples.
4. Isolate process-global loggers and environment changes. Independent integration
   binaries are useful for logging and subprocess tests; library test modules share
   a process and must not install conflicting global loggers.
5. Native tests operate only on fixture-owned windows. Use hidden or nonactivating
   offscreen windows and explicit cleanup on the owning thread. Never depend on the
   user having a particular application open or mutate unrelated desktop windows.
6. Avoid wall-clock sleeps for timing rules. Test a private time-parameterized predicate
   through a seam and use real native integration tests only for actual native contracts.
7. Keep source API documentation and intent comments near their implementation. Put
   regression assertions and test scenarios under `tests/`. Do not introduce executable
   source doc-test bodies as a workaround for the physical test-code separation rule.
8. Follow repository style manually, including four-space indentation. Do not run a
   formatter as part of test migration.

## Running and checking discovery

```nu
cargo test -p turbozone-core --release --locked
cargo test -p turbozone-core --release --locked --lib engine::test
cargo test -p turbozone-core --release --locked --test logging
cargo test --workspace --release --locked -- --list
cargo test --workspace --release --locked
cargo check --workspace --all-targets --release --locked
cargo clippy --workspace --all-targets --all-features --release --locked -- -D warnings
```

After moving files, inspect discovery output for both included unit suites and public
integration targets. Verify that helper files are not independent empty targets and
that the production build does not include extension traits or fixtures. Keep Cargo
dev-dependencies sufficient for unit tests; do not promote test dependencies to normal
dependencies merely because their source modules are included from a production file.

Documented private seam names and test locations may evolve with their owning modules;
the enduring contract is physical separation, narrow test-only access, and preserved
behavioral coverage.
