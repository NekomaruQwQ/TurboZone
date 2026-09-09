# Proposal of Change: ordered snapshot outcomes

Status: approved on 2026-09-09. Implemented and validated.

This follow-up replaces the split snapshot representation introduced by
[the initial refactor](PoC-SnapshotRefactor.md). The working copy was clean at the
start, on top of `d4a23f57`. The backend retains one batch `snapshot()` operation.

## Contract and responsibilities

The backend returns every included handle and its capture result in enumeration
order. The engine owns that sequence and derives groups and error entries on demand.
This retains the relative order of successful and failed captures together, which
the previous success-vector/error-map partition could not represent.

```rust
fn snapshot(&mut self)
 -> anyhow::Result<Vec<(Self::Handle, anyhow::Result<WindowInfo<Self::Handle>>)>>;

pub struct EngineView<'a, H> {
    pub groups: Vec<Group<'a, H>>,
    pub errors: Vec<(H, &'a anyhow::Error)>,
}

pub fn group_windows<'a, H: 'a>(
    rules: &'a [Rule],
    windows: impl IntoIterator<Item = &'a WindowInfo<H>>) -> Vec<Group<'a, H>>;
```

The `Snapshot` struct is removed; there is no replacement alias or wrapper.
`Engine::snapshot()` borrows the same outer Result and vector returned by the backend.
The vector is the engine's only owned representation of the current observation.

1. Each included handle occurs exactly once. A successful tuple's handle equals
   `WindowInfo.handle`. This remains a backend invariant, rather than a new engine
   normalization or duplicate-resolution policy.
2. An outer error invalidates the entire enumeration. Inner errors preserve their
   original causes beside their handles. Backend-excluded windows remain absent.
3. Backend filtering, required-query order, snapshot-local monitor/process caches,
   and persistent program-cache retention remain unchanged. No timer is introduced.
4. Group members borrow successful windows in their relative enumeration order.
   Error entries copy handles and borrow errors in their relative enumeration order.
   Group ordering remains rule source order, then lowercase program path. The first
   member continues to supply representative program metadata.
5. Native enumeration order can change between calls. Preserving each returned
   sequence does not promise stable order across refreshes.

## Data flow and isolated mechanisms

For `[A: success, B: failure, C: success]`, the Windows backend retains all three
entries in that order after its relevance filters. The engine stores the sequence.
An EngineView considers A and C for rule matching and borrows B's error; unmatched
successes remain available through raw snapshot access.

Grouping accepts borrowed iterators so filtering successes requires no intermediate
window list. Views allocate their group membership and error-reference vectors;
they do not clone windows, errors, or program Rc values and make no native queries.

On update, the engine builds a temporary borrowed lookup of previous errors by
handle, then checks current failures in enumeration order. This keeps comparison
expected linear in the number of entries, independent of entry positions, without
introducing another persistent error cache. Full-chain comparison remains isolated
in `same_error`. Reordering does not produce new logs. Recovery, disappearance,
enumeration outages, and unconditional action-failure logging retain their semantics.
Newly emitted per-window warnings now follow enumeration order.

The UI still captures failed-handle Display descriptions and full error text once
per update and sorts its diagnostic rows by description. Usable rows and group
action targets preserve member order. Enumeration failure, mixed results,
errors-only, empty, and recovery presentation retain their behavior.

Local iterator construction, native capture helpers, and test fixture adaptations
are delegated implementation details within these contracts.

## Alternatives and scope

A HashMap would encode unique keys but discard enumeration order. An insertion-ordered
map would add an unnecessary collection dependency for this sequential capture flow.
The chosen vector expresses the requested order directly using standard containers.
Separating enumeration and capture or introducing cache expiration changes lifecycles
and is outside this proposal.

The coordinated change covers core engine/data documentation, the Windows backend,
the UI adapter, their existing private extension seams and test suites, and the
design overview. Snapshot fixture literals become ordered `(handle, result)` entries;
field accesses become outcome selection. Tests remain physically under `tests/`.
Existing CatIgnore rules already cover every new or broadly changed path.

## Implementation reconciliation

The direct vector contract, borrowed EngineView error entries, iterator-based
grouping, temporary error lookup, and consumer migrations are implemented without
architectural deviations. The Windows backend no longer wraps errors in a second
handle-bearing Result because the tuple now retains identity on either outcome.

Clippy type-complexity expectations are scoped to the explicit nested-result
contract and scripted snapshot fields, with reasons beside each expectation.
They preserve the approved direct representation without introducing a type alias.

Regression coverage includes interleaved successes/failures and unmatched windows,
non-sorted handle order, original window/error borrow identity and native causes,
deduplication after reordering, newly emitted warning order, successful tuple identity,
UI row and group-action order across a failed entry, and description-sorted diagnostics.
Existing failure/recovery, filtering, geometry, and action tests are migrated.

Validation commands:

```text
cargo test --workspace --release --locked --offline
cargo check --workspace --all-targets --release --locked --offline
cargo clippy --workspace --all-targets --all-features --release --locked --offline -- -D warnings
```

All 122 workspace tests pass with zero failures or ignored tests. Workspace check and
Clippy with warnings denied pass. Source scans find no remaining Snapshot type or
split-field access and no test bodies under source directories. Existing CatIgnore
rules cover all new and broadly changed files; no policy edits are needed.

Formatting is manual, with existing line endings preserved. No formatter ran and no
dependencies changed. Native tests operate on fixture-owned windows; desktop appearance
was not manually reviewed. There are no unresolved implementation concerns.
