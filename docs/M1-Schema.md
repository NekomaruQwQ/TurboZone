# TOML editor support

The checked-in [turbozone.schema.json](turbozone.schema.json) is generated from
`turbozone_core::Config` and its nested types. Rust types, Serde attributes, and
Rust doc comments supply field names, optionality, alternatives, and hover documentation.
The TypeScript declarations in this directory are illustrative, not generator input.

## Use in an editor

1. Enable a TOML language server with JSON Schema support, such as
   [Tombi](https://tombi-toml.github.io/tombi/docs/json-schema/) or
   [Taplo / Even Better TOML](https://taplo.tamasfe.dev/configuration/using-schemas.html).
2. Place `turbozone.schema.json` beside your configuration, or adjust the relative path
   in this comment at the beginning of the TOML file:

   ```toml
   #:schema ./turbozone.schema.json

   [[rules]]
   name = "vscode.main"
   move = true
   program.name = "Code.exe"
   resize.exact = [1440, 900]
   ```

The path is relative to the TOML file, not the workspace. The
[example configuration](M1-Plan-config.toml) already has the directive. A local schema
works offline; keep the schema from the same revision as the app when distributing it.
Do not add a `$schema` TOML key: unknown configuration fields are rejected by Serde.

For a quick editor check, request completion inside `[[rules]]`, hover `move`, and
try an unknown key or `resize.exact = [0, 900]` to check diagnostics. An exact resize
cannot be combined with selector keys such as `min` or `default`.

## Regenerate and verify

Run from the repository root:

```sh
cargo run --release -p turbozone-core --features schema --example config-schema
cargo test --release --workspace --all-features --locked
cargo clippy --release --workspace --all-targets --all-features --locked -- -D warnings
```

The generator writes UTF-8 JSON directly to `docs/turbozone.schema.json`, independent
of the shell's redirection encoding. Commit the regenerated file with config type or
documentation changes. The freshness test compares parsed JSON, so platform line endings
do not cause failures, but a stale schema does. Regeneration is explicit; normal builds
do not rewrite source files.

The optional `schema` feature enables Schemars derives and schema generation only.
Normal application builds do not enable it. `serde_json` is a development dependency
for the generator and tests. The output uses JSON Schema Draft 7 and internal references,
without additional schemas to download. Schemars updates can change the generated output;
review the diff when updating the lockfile.

## Validation boundary

The schema describes the input structure, including Serde defaults, renamed keys,
untagged alternatives, and rejected unknown fields. Sizes use `[i32; 2]`, serialized
as `[width, height]` in physical pixels. Schemars derives the fixed array length;
per-element range annotations require positive integers no greater than `i32::MAX`.
No geometry-specific schema adapter is needed.

The TOML 1.1.4 reader currently ignores extra dimensions in `window.min` and `window.max`,
as it did with Euclid sizes. The schema rejects those arrays; runtime validation cannot
detect elements already discarded by deserialization.

`Config::validate()` remains authoritative for rule-name grammar, duplicate names,
nonempty partial patterns, forward slashes in program paths, and `min <= max` comparisons.
Editor validation does not replace those checks. Optional Rust fields may include JSON
`null` in the schema; TOML itself has no null value, so omit those fields instead.
