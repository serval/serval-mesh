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
    cargo clippy --all-targets
    cargo fmt --check

# Run tests with nextest
test:
    @cargo nextest run --all-targets

# Lint and automatically fix what we can fix
@lint:
    cargo fmt --all
    cargo clippy --all-targets --fix --allow-dirty --allow-staged

# Cargo install required tools like `nextest`
@install-tools:
    cargo install cargo-nextest

# Everyone loves Lady Gaga, right?
@dance:
    open "https://www.youtube.com/watch?v=2Abk1jAONjw"
