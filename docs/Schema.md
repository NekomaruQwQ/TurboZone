# TOML editor support

`turbozone_core::Config` derives its JSON Schema from Rust types, Serde attributes, and Rust doc
comments. These supply field names, optionality, alternatives, and hover text. The TypeScript
declarations in this directory are illustrative, not generator input.

## Explicit config source

Pass a file explicitly, or set `TURBOZONE_CONFIG`. The CLI wins when both are supplied:

```sh
cargo run --release -p turbozone-windows --bin turbozone -- --config D:/Private/TurboZone/local.config.toml
```

Relative paths use the executable's containing directory, independent of the launcher's working
directory. There is no implicit default or search path; use an absolute path when private config
lives elsewhere. Parent directories must already exist, and the app creates only the config file
itself when it is missing.

Startup never generates or updates schemas. Maintainers regenerate the checked-in canonical schema
explicitly from the repository root:

```sh
cargo test --release -p turbozone-core --test generate_schema
```

## Use in an editor

1. Enable a TOML language server with JSON Schema support, such as
   [Tombi](https://tombi-toml.github.io/tombi/docs/json-schema/) or
   [Taplo / Even Better TOML](https://taplo.tamasfe.dev/configuration/using-schemas.html).
2. A newly created config contains only its schema directive and is a valid empty configuration:

   ```toml
   #:schema https://raw.githubusercontent.com/NekomaruQwQ/TurboZone/refs/heads/main/data/config.schema.json
   ```

3. Existing configs are never rewritten automatically. To enable schema support, add or update
   the directive yourself, then add rules as needed:

   ```toml
   #:schema https://raw.githubusercontent.com/NekomaruQwQ/TurboZone/refs/heads/main/data/config.schema.json

   [[rules]]
   name = "vscode.main"
   move = true
   program.name = "Code.exe"
   resize.exact = [1440, 900]
   ```

The [example configuration](config.example.toml) references the canonical schema on the main
branch. Editor validation therefore requires access to that URL. Do not add a `$schema` TOML key:
unknown top-level fields are fatal; `#:schema` is a comment understood by the editor.

For an editor check, request completion inside `[[rules]]`, hover `move`, and try an unknown key
or `resize.exact = [0, 900]`. An exact resize cannot be combined with selector fields such as
`min` or `default`. Runtime errors appear in the terminal rather than an in-app diagnostics UI.

## Schema and parser boundaries

The schema uses JSON Schema Draft 2020-12 with `$defs` internal references, without additional downloads.
It describes input structure, defaults, renamed keys, untagged alternatives, and rejected unknown
fields. Sizes use `[i32; 2]`, serialized as `[width, height]` in physical pixels. Schemars derives the
fixed array length; per-element annotations require integers from `1` through `16,384`.
Optional Rust fields can include JSON `null` in the schema; TOML has no null, so omit them instead.

`turbozone-core::parse_config()` deserializes the complete document before semantic validation.
Invalid TOML, invalid top-level structure, or any invalid rule rejects the complete configuration;
partially compiled rules are never exposed. Valid rules keep their relative order, and empty config
is valid.

The compiler checks rule-name grammar, duplicates, nonempty partial patterns, forward slashes
in program paths, dimensions from `1` through `16,384`, and `min <= max`. It performs no I/O;
each failure is logged at error level without TOML source excerpts before parsing returns no runtime
configuration. Editor checks do not replace runtime validation.

## Verify

```sh
cargo test --release -p turbozone-core --test generate_schema
cargo test --release --workspace --all-features --locked
cargo clippy --release --workspace --all-targets --all-features --locked -- -D warnings
```

The dedicated generator test rewrites `data/config.schema.json`; the remaining tests compare schema
semantics with serialization and runtime behavior and verify that startup leaves existing config
and sibling files untouched. Schemars updates can alter the generated output, so review schema
behavior when updating the lockfile.
