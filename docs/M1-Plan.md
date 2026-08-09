# M1 Plan

## Proposal of Change v3

### Configuration

Every rule targets a named group directly by stable ID:

```toml
[[groups]]
id = "edge-pwa"
name = "My Edge PWA"
allow_resize = true
default_size = [1440, 900]

[[groups]]
executable.name = "msedge.exe"
allow_resize = true
default_size = [1920, 1200]

[[rules]]
name = "My Edge PWA"
group = "edge-pwa"
executable.name = "msedge.exe"
window_title.contains = "My PWA"
```

There is no embedded `{ named = ... }` object.

Group definitions have two mutually exclusive forms:

- Named group: `id` and display `name`.
- Executable policy: `executable.*`, applied separately to every matching executable.

Rules always target named groups. Unmatched windows group automatically by executable and obtain properties from the first matching executable policy.

Runtime representation:

```text
named groups          → BTreeMap<GroupId, NamedGroup>
executable policies   → ordered Vec<ExecutableGroupPolicy>
rules                 → ordered Vec<Rule>
resolved groups       → BTreeMap<ResolvedGroupId, WindowGroup<T>>
```

### Matching

Supported string matchers remain:

```toml
executable.name = "msedge.exe"
executable.path = "C:/Program Files/Microsoft/Edge/msedge.exe"
executable.path.contains = "/Microsoft/Edge/"
window_title = "Exact title"
window_title.contains = "Contained title"
```

Semantics:

- Rule fields are ANDed.
- Rules are first-match-wins.
- Executable policies are first-match-wins.
- Executable paths and filenames compare case-insensitively.
- Window titles remain case-sensitive.
- Missing fields are unconstrained.
- Empty `contains` values are rejected.

### Path normalization

Only paths originating from Win32 are normalized. Config values are consumed exactly as written.

The native-path pipeline is:

```rust
path
    .normalize_lexically()
    .to_string_lossy()
    .replace('\\', "/")
```

The exact unstable method name is `Path::normalize_lexically`; lexical normalization necessarily happens while the value is still a `Path`, before `to_string_lossy`.

This intentionally:

- Resolves lexical `.` and `..` components.
- Avoids filesystem access and canonicalization.
- Converts Win32 backslashes to config-style slashes.
- Accepts lossy conversion for unusual non-Unicode executable paths.
- Preserves the casing returned by Win32; comparison handles case insensitivity.

Config paths are not normalized or silently corrected. Any backslash in `executable.path` is a validation error, making the forward-slash contract visible instead of hiding mistakes.

### Size behavior

Resize history is removed from this version.

- `default_size` is the configured one-click target.
- Selecting another size from the GUI applies it once but does not persist it.
- The next primary resize action still uses `default_size`.
- If `allow_resize = true` but no default exists, the control opens the size list rather than inventing a target.
- `default_size` with `allow_resize = false` is invalid configuration.

`TurboRnR.history.toml` is therefore reserved but neither created nor read in this iteration. Only `TurboRnR.config.toml` is active.

### Euclid in `turbo-layout-core`

`turbo-layout-core` uses euclid for all geometry:

```rust
pub type WindowSize = euclid::default::Size2D<i32>;
```

The TOML representation remains:

```toml
default_size = [1440, 900]
```

A small custom serde adapter converts `[i32; 2]` to and from `Size2D<i32>`. This avoids creating a duplicate width/height type purely for serialization and keeps the engine consistently euclid-based.

Geometry validation rejects non-positive dimensions before configuration becomes runtime state.

### Crate responsibilities

- `turbo-layout-core`
  - Euclid geometry.
  - Config types and validation.
  - Exact/contains matchers.
  - Named/executable group resolution.
  - Generic `WindowGroup<T>`.
  - No native calls or Windows types.

- `turbo-layout-windows`
  - Win32 enumeration and manipulation.
  - Native path normalization.
  - Executable metadata.
  - HWND/PID snapshots.
  - Centering and resizing.

- `turbo-layout-windows-app`
  - `TurboRnR.exe`.
  - `<exeName>.config.toml` discovery.
  - TOML file loading and error presentation.
  - TurboRun-derived GUI.
  - Queued native actions.

No backward compatibility or migration logic will be included. The old config remains untouched on disk but is ignored.

### Important tests

The core test suite will cover:

- Rule references to valid and missing named IDs.
- Duplicate named IDs.
- Exact and contains deserialization.
- Case-insensitive path and filename matching.
- Case-sensitive title matching.
- Backslash rejection in configured paths.
- Win32 path normalization, including `.` and `..`.
- First-match rule and executable-policy precedence.
- One executable policy producing several independent groups.
- Multiple executables intentionally joining one named group.
- Unmatched executable fallback.
- Unknown-path grouping.
- Invalid and valid euclid sizes.
- `allow_resize`/`default_size` invariants.
- Alternative UI sizes not changing the configured default.

### Implementation order

1. Rebrand and convert the repository into the three-crate workspace.
2. Move pure geometry/data into `turbo-layout-core`, preserving behavior.
3. Move Win32 operations into `turbo-layout-windows` and introduce the specified path normalization.
4. Implement configuration deserialization, validation, and grouping tests.
5. Integrate rules and executable policies.
6. Port TurboRun’s style and unified collapsible cards.
7. Remove legacy config/history code and unused dependencies.
8. Run the workspace tests and Clippy, then perform visual QA without automated formatting.
