# serval-mesh

[![Main branch checks](https://github.com/servals/serval-mesh/actions/workflows/main.yaml/badge.svg)](https://github.com/servals/serval-mesh/actions/workflows/main.yaml)

This monorepo contains the source for the various components of the Serval mesh, intended to run on any host where you want to run WASM payloads.

- `castaway`: temporarily a separate service during MVP/demo period, this stores blobs for the Serval mesh. This functionality will eventually be integrated into the Serval agent.
- `engine`: the [wasmtime](https://lib.rs/crates/wasmtime) glue
- `test-runner`: a CLI to execute a WASM payload once, useful for developing the engine
- `serval-agent`: a daemon that listens on a port for incoming HTTP requests with payloads to run
- `queuey-queue`: temporarily a separate service during MVP/demo period, this is manages the job queue for the Serval mesh. This functionality will eventually be integrated into the Serval agent.

To build everything: `just build`. To test everything: `just ci`. `just help` to see other justfile recipes.

## LICENSE

[BSD-2-Clause-Patent](./LICENSE)
