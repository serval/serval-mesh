# List available recipes
help:
    just -l

# Run a test, assuming the example repo is one level up...
test EXEC="loudify" IN="README.md":
    cargo run -- ../wasm-samples/build/{{EXEC}}.wasm {{IN}}
