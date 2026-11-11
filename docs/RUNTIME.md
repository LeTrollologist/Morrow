# Runtime prerequisites

Morrow's release gate builds every example and executes the supported set with
expected exit codes. A few examples have external prerequisites and are therefore
build-only in `scripts/pipeline.py`:

| Example | Why build-only |
|---|---|
| `colorless_network_server.mw` | Long-running TCP server; requires a client connection. |
| `http_server.mw` | Long-running HTTP listener; requires an HTTP client. |
| `tcp_echo_server.mw` | Interactive echo server. |
| `sqlite_benchmark.mw` | Requires `sqlite3.dll` at runtime. |
| `web_service.mw` | Requires `sqlite3.dll` at runtime. |

## SQLite

`lib/crt/libsqlite3.a` is an import library. The matching `sqlite3.dll` is **not**
shipped in the release archive. To run the SQLite examples, place a compatible
64-bit `sqlite3.dll` next to the generated executable or on `PATH`. Without it,
Windows terminates the process with `STATUS_DLL_NOT_FOUND` (`0xC0000135`).

The parameterized driver (`std::sqlite`) binds values; `std::sql_builder` only
composes caller-supplied SQL fragments and is not a sanitizer.

## Expected exit codes

`wasm_sample.mw` intentionally returns `55` (the tenth Fibonacci number). Every
other executed example must return `0`. The gate records pass/fail/skip details in
`target/pipeline-examples.log`.
