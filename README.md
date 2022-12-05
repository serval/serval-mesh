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
0a2ad9799736639c67c79e2be0d188afdaf7816d%
```
### Store raw text 
```
$ curl localhost:7475/blob -XPUT -d 'WILSON!'
68d5b57378f76645bf12506c88fb06a81ddb3965
```
### Retrieve content
```
$ curl localhost:7475/blob/68d5b57378f76645bf12506c88fb06a81ddb3965
WILSON!
```
