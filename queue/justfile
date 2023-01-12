# List available recipes
help:
    @just -l

# Run tests with nextest
test:
    @cargo nextest run

# Run the same checks we run in CI
@ci: licenses
    cargo test
    cargo clippy --all-targets
    cargo fmt --check

# Lint and automatically fix what we can fix
@lint:
    cargo fmt
    cargo clippy --all-targets --fix --allow-dirty --allow-staged

# Vet dependency licenses
@licenses:
    cargo deny check

# Cargo install required tools like `nextest`
@install-tools:
    cargo install cargo-nextest
    cargo install cargo-deny
