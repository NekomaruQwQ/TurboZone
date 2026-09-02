# TOML editor support

`turbozone_core::Config::schema()` generates the JSON Schema from Rust types, Serde attributes,
and Rust doc comments. These supply field names, optionality, alternatives, and hover text.
The TypeScript declarations in this directory are illustrative, not generator input.

## Explicit config source

Pass a file explicitly, or set `TURBOZONE_CONFIG`. The CLI wins when both are supplied:

```sh
cargo run --release -p turbozone-windows --bin turbozone -- --config D:/Private/TurboZone/local.config.toml
```

Relative paths use the current working directory. There is no executable-adjacent default or
search path. This lets private config live in its own local versioned directory. Parent directories
must already exist; the app creates only the config file itself when it is missing.

On startup, the app first writes the current schema to `config_path.with_extension("schema.json")`.
For `local.config.toml`, this is `local.config.schema.json` beside it. Schema-write failure is a
warning and loading continues. The schema is replaceable generated data; it is not a checked-in
artifact and no standalone generation feature or example is required.

## Use in an editor

1. Enable a TOML language server with JSON Schema support, such as
   [Tombi](https://tombi-toml.github.io/tombi/docs/json-schema/) or
   [Taplo / Even Better TOML](https://taplo.tamasfe.dev/configuration/using-schemas.html).
2. A newly created config contains only its schema directive and is a valid empty configuration:

   ```toml
   #:schema ./local.config.schema.json
   ```

3. Existing configs are never rewritten automatically. To enable schema support, add or update
   the directive yourself, then add rules as needed:

   ```toml
   #:schema ./local.config.schema.json

   [[rules]]
   name = "vscode.main"
   move = true
   program.name = "Code.exe"
   resize.exact = [1440, 900]
   ```

The schema path is relative to the TOML file. The [example configuration](config.example.toml)
already names its generated sibling. Local schemas work offline. Do not add a `$schema` TOML
key: unknown top-level fields are fatal; `#:schema` is a comment understood by the editor.

For an editor check, request completion inside `[[rules]]`, hover `move`, and try an unknown key
or `resize.exact = [0, 900]`. An exact resize cannot be combined with selector fields such as
`min` or `default`. Runtime warnings appear in the terminal rather than an in-app diagnostics UI.

## Schema and parser boundaries

The schema uses JSON Schema Draft 2020-12 with `$defs` internal references, without additional downloads.
It describes input structure, defaults, renamed keys, untagged alternatives, and rejected unknown
fields. Sizes use `[i32; 2]`, serialized as `[width, height]` in physical pixels. Schemars derives the
fixed array length; per-element annotations require positive integers no greater than 8192.
Optional Rust fields can include JSON `null` in the schema; TOML has no null, so omit them instead.

`turbozone-core::parse_config()` parses the document and compiles rules independently. Invalid
TOML and invalid top-level structure are fatal. Individual rule errors exclude only that rule;
valid rules keep their relative order. Diagnostics use original zero-based rule indices. Only
the first valid rule reserves its name, and later duplicates are skipped. Empty config is valid.

The compiler checks rule-name grammar, duplicates, nonempty partial patterns, forward slashes
in program paths, positive dimensions, and `min <= max`. `compile_config()` offers the same
compiler for callers that already have a typed `Config`. Neither function performs I/O or logs;
the executable reports diagnostics without TOML source excerpts. Editor checks do not replace
runtime validation.

## Verify

```sh
cargo test --release --workspace --all-features --locked
cargo clippy --release --workspace --all-targets --all-features --locked -- -D warnings
```

Tests compare generated JSON with serialization and runtime behavior, and verify that startup
refreshes the sibling schema without changing existing config bytes. Schemars updates can alter
the generated output; review schema behavior when updating the lockfile.
