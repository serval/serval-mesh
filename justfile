# List available recipes
help:
    just -l

# Build all targets in debug mode
@build:
    cargo build --release --all-targets

# Build all targets in release mode
@release:
    cargo build --release --all-targets

# Build documentation for all crates
@doc *FLAGS:
    cargo doc --release --no-deps --workspace {{FLAGS}}

# Run the same checks we run in CI
@ci: test
    cargo clippy --workspace
    cargo +nightly fmt --check
    cargo deny check licenses

# Get security advisories from cargo-deny
@security:
    cargo deny check advisories

# Run tests with nextest
@test:
    cargo nextest run --all-targets

# Lint and automatically fix what we can fix
@lint:
    cargo clippy --fix --allow-dirty --allow-staged --all-targets
    cargo +nightly fmt --all

# Install required linting/testing tools via cargo.
@install-tools:
    cargo install cargo-nextest
    cargo install cargo-deny

# Check for unused dependencies.
check-unused:
    cargo +nightly udeps --all

# Everyone loves Lady Gaga, right?
@dance:
    open "https://www.youtube.com/watch?v=2Abk1jAONjw"


@run-dev:
    zellij --layout dev-layout.kdl

tailscale *ARGS:
    just tailscale-run cargo run --bin serval-agent -- {{ARGS}}

tailscale-monitor *ARGS:
    just tailscale-run cargo run --bin serval -- monitor {{ARGS}}

tailscale-run *CMD:
    #!/usr/bin/env bash
    if [ $(uname) == "Darwin" ]; then
        echo "Serval doesn't currently work well when run against Tailscale using IPv6; aborting."
        exit 1
    fi

    ADDR=$(tailscale ip --6 2>/dev/null)
    if [ "$?" -ne 0 ]; then
    echo This command requires the Tailscale CLI tool to be installed:
        echo https://tailscale.com/kb/1080/cli/
        exit 1
    fi
    if [ "${ADDR}" == "" ]; then
        echo Tailscale does not have an IPv6 address; aborting.
        exit 1
    fi

    CROSSWIND_PID=$(pgrep crosswind)
    if [ "$?" -ne 0 ]; then
        echo "💨 Running Serval over your tailnet requires crosswind to be running; check out" >&2
        echo "💨 https://github.com/serval/crosswind and run \`just tailscale\` if you haven't already." >&2
        echo "" >&2
    fi

    MESH_INTERFACE="${ADDR}" {{CMD}}
