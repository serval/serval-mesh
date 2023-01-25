# queuey-queue

This is a basic job queue for the Serval mesh; we intend to replace it with something better as time
permits.

In general, the job queue has the following responsibilities:

- accepting jobs to be enqueued
- distributing jobs to worker nodes when they request one
- updating the "last worked on" timestamp of active jobs
- detecting unsuccessful job runs and putting the job back in the queue
- accepting the results of successful job runs
- allowing the status of jobs to be queried
- allowing the results of successful job runs to be collected

## Jobs

A `job` is a request for work to be done. Jobs are created by giving the job queue
[the CAS address](https://github.com/serval/castaway) of a WASI binary to run, as well as the CAS
address of input data to provide to that binary as its stdin.

### Job statuses

- `pending`: the job is waiting to be claimed by a worker
- `active`: the job has been claimed by a worker, which is presumably doing something with it
- `completed`: the job has been successfully completed by a worker
- `failed`: the job was explicitly marked as failed by the worker that owned it, or it was abandoned
  `MAX_ATTEMPTS` times and will not be automatically tried again

## Constants

- `ABANDONED_AGE` = 300. How many seconds since `updated_at` before we consider a job to be abandoned.
- `MAX_ATTEMPTS` = 3. Maximum number of times to retry abandoned jobs before marking them as failed.

## Error handling and retries

Jobs all have an `updated_at` timestamp associated with them. Runners are required to call the
`/jobs/:id/tickle` endpoint periodically; active jobs with an `updated_at` timestamp more than
`ABANDONED_AGE` seconds in the past will be considered abandoned.

When a job is abandoned,

- `updated_at` is set to the current timestamp
- `runner_id` is cleared
- `run_attempts` is incremented
- if `run_attempts` <= `MAX_ATTEMPTS`, status is set to `pending`
- if `run_attempts` > `MAX_ATTEMPTS`, status is set to `failed`

## Discovery

queuey-queue advertises itself over mDNS under the namespace `_serval:queue._tcp.local.`. Its
properties include an `http_port` field telling consumers how to talk to it.

## API

YAML is used at the moment in these examples for readability, but queuey-queue uses json as its payload format.

### `POST` `/jobs/create`

Enqueues a new job, which will be created with the status of `pending`.

Parameters:

- `binary_addr`: a CAS address pointing to a WASM binary on the Storage Server
- `input_addr`: optionally, a CAS address pointing to a blob on the Storage Server

Example output:

```yaml
job_id: F49B1EBD-EDF4-4839-9EDB-EF3082D32C14
binary_addr: 59ae214373240a255f453cc2fa8d26ab60d6b532
input_addr: 7a293b5b7ac61a1691848e375a110f19de3de698
created_at: 1670273606123
updated_at: 1670273606456
completed_at:
status: pending
run_attempts: 0
runner_id:
```

### `GET` `/jobs/:id`

Returns the given job.

Example output:

```yaml
job_id: F49B1EBD-EDF4-4839-9EDB-EF3082D32C14
binary_addr: 59ae214373240a255f453cc2fa8d26ab60d6b532
input_addr: 7a293b5b7ac61a1691848e375a110f19de3de698
created_at: 1670273606123
updated_at: 1670273663456
completed_at: 1670275321789
status: completed
run_attempts: 1
runner_id: A54FAF6A-8C15-4BC0-8C1D-49EBA31AD550
```

### `POST` `/jobs/claim`

Finds the oldest job in the `pending` state and updates it to be claimed by whichever runner is
hitting this endpoint. Changes the status to `active` and increments `run_attempts`.

Parameters:

- `runner_id`: the instance of the runner that is trying to claim a job

Example output:

- see example output for `GET` `/jobs/:id`

### `POST` `/jobs/:id/tickle`

Sets `updated_at` timestamp for the current job to the current time, if and only if the job is `active`. Returns
a 404 (if the job does not exist) or 400 (if it exists but is not `active`).

### `POST` `/jobs/:id/complete`

Parameters:

- `status`: either `completed` or `failed`.
- `output_addr`: CAS address of where the job's output was stored.

## Repository layout

This is a Rust project. To build, run `cargo build`. To run locally, `cargo run`. To run the tests, `cargo test`. If you do not have the rust compiler available, install it with [rustup](https://rustup.rs).

A [justfile](https://just.systems) is provided for your convenience. It defines these recipes:

```text
➜ just -l
Available recipes:
    ci            # Run the same checks we run in CI
    help          # List available recipes
    install-tools # Cargo install required tools like `nextest`
    licenses      # Vet dependency licenses
    lint          # Lint and automatically fix what we can fix
    test          # Run tests with nextest
```

Documentation of interest to _implementors_ is in-line in the source and viewable with `cargo doc --open`. Full API documentation and other user docs are in the [docs](./docs/) subdirectory.

# LICENSE

[BSD-2-Clause-Patent](./LICENSE)
