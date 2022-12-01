# CAStaway

This is a temporary, throwaway CAS-based storage system. Hence, CAStaway. Get it?

## Usage

`cargo run --storage-path /place/to/keep/the/files`

## API

- `PUT /blob`

  Stores a blob. Returns the `{ "address": "..." }` it is stored at.

- `GET /:address`

  Returns the bytes of the given blob, or 404 if not found.
