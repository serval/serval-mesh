# List available recipes
help:
    just -l

# Build all targets in debug mode
@build:
    cargo build --release --all-targets

# Build all targets in release mode
@release:
    cargo build --release --all-targets

# Run the same checks we run in CI
@ci: test
    cargo clippy --all
    cargo fmt --check --all
    cargo deny check licenses

# Get security advisories from cargo-deny
@security:
    cargo deny check advisories

# Run tests with nextest
test:
    @cargo nextest run --all-targets

# Lint and automatically fix what we can fix
@lint:
    cargo clippy --all --fix --allow-dirty --allow-staged
    cargo fmt --all

# Cargo install required tools like `nextest`
@install-tools:
    cargo install cargo-nextest
    cargo install cargo-deny

# Everyone loves Lady Gaga, right?
@dance:
    open "https://www.youtube.com/watch?v=2Abk1jAONjw"

