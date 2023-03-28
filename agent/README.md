# Serval Agent

The serval agent is a persistent process that advertises itself on the network as a runner for Wasm jobs. It listens on HTTP for incoming job requests.

It is _not yet_ a full Serval agent node, because it does not make any attempt to find a control node and ask for jobs. Instead it listens passively to be pushed incoming workloads.

It has _no persistent storage_ at the moment.

## API sketch

### `GET /monitor/ping`

Response is 200 plus a short string to indicate liveness. The exact contents of the string may vary and should *not* be depended upon.

### `GET /monitor/history`

TODO; this should respond with a history of jobs and their statuses

### `POST /jobs`

This endpoint is not likely to remain in its current state; it exists to allow any HTTP client to post a test job to the runner.

This endpoint accepts an executable Wasm job via multipart form data. The body must include two parts: a job metadata envelope in json format named `envelope`, and an octet-stream containing the Wasm binary to run.

Here is an OpenAPI schema definition for this request:

```yaml
requestBody:
  content:
    multipart/form-data:
      schema:
        type: object
        properties:
          envelope:  # an envelope containing metadata for the job; the props are speculative
            type: object
            properties:
              name:
                type: string
              id:
                type: string
                format: uuid
              description:
                type: string
              update_url: # the tickle url; need a better name
                type: string
                format: url
              results_url: # where to send the results
                type: string
                format: url
              # probably more goes here
          executable:  # the wasm binary, as an octet-stream
            type: string
            format: binary
```

For the moment, this endpoint responds with `202 Accepted` and a URL to poll for updates.

### `GET /jobs/:id/status` UNIMPLEMENTED

This endpoint responds with the status of a job.

```
Enum {
    Unknown,
    Pending,
    Running,
    Complete
}
```

### `GET /jobs/:id/result` UNIMPLEMENTED

Responds with job results. Format TBD.
