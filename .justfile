set shell := ["nu", "-l", "-c"]

# Run TurboZone.
run *args:
    cargo run -r --bin turbozone -- {{args}}

# Run TurboZone in development mode, loading the config from data/config.toml.
dev *args:
    cargo run -r --bin turbozone -- {{args}} -c $"($env.PWD)/data/config.toml"

# Generate JSON schemas to data/.
generate-schema:
    cargo test -r --bin generate_schema
