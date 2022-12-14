# CAStaway

This is a temporary, throwaway CAS-based storage system. Hence, CAStaway. Get it?

https://en.wikipedia.org/wiki/Content-addressable_storage

## Usage

`cargo run --path /place/to/keep/the/files`

## API

- `PUT /blob`

  Stores a blob. Returns the `{ "address": "..." }` it is stored at.

- `GET /:address`

  Returns the bytes of the given blob, or 404 if not found.

## Examples

### Store a file

```bash
$ curl localhost:7475/blob -sT README.md
eef0465653bf02714b54a0b15da7e5146a98037f9d5ddff777591fd21b0c42eb%
```

### Store raw text

```
$ curl localhost:7475/blob -XPUT -d 'WILSON!'
979f59d88a9ea9a2ce524f679861133cdfb7318570302f3147f129a12f2e9698
```

### Retrieve content

```
$ curl localhost:7475/blob/68d5b57378f76645bf12506c88fb06a81ddb3965
WILSON!
```

## Licensing policy

This project should not pull in any crates with incompatible licensing terms.
Run `cargo deny check licenses` after adding or upgrading dependencies to ensure we are in
compliance with our licensing policy.
