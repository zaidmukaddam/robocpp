# IDE/WASM Integration Contract

`iec_language_service_wasm` is the stable boundary between compiler internals and
`ide/web`. Compiler refactors may change internals freely, but they must keep this
contract stable unless the IDE migration lands in the same change.

## Stable Exports

The IDE currently consumes these JSON exports:

- `analyze_document_json(uri, text, languageId)`
- `graph_model_json(uri, text, languageId)`
- `validate_graph_json(uri, text, languageId)`
- `run_document_json(uri, text, languageId, cycles)`
- `debug_document_json(uri, text, languageId, cycles)`
- `generated_c_artifact_json(uri, text, languageId)`
- `capabilities_json()`

Additive exports are allowed. Renaming or removing one of the exports above is an
IDE-breaking change.

## Stable JSON Shape

The fixture in `tests/fixtures/ide_contract_schema.json` records the top-level
keys and nested keys the IDE depends on. Field additions are allowed. Removing or
renaming recorded fields is an IDE-breaking change unless `ide/web` is migrated
in the same patch.

The especially sensitive fields are diagnostics, completions, symbols, graph
models, graph validation diagnostics, debug cycles, generated-C artifact metadata,
capabilities, PLCopen graph IDs, connector IDs, geometry, and vendor metadata.

## Local Fallback Alignment

`ide/web/src/services/wasmClient.ts` falls back to local TypeScript
implementations when WASM exports are unavailable or fail. Those fallback result
shapes must stay aligned with this contract so the React UI can switch between
`wasm` and `local` engine modes without component-level branching.

## Verification

Run this before changing compiler internals that feed the IDE:

```sh
cargo test -p iec_language_service_wasm
```

For PLCopen DOM-lowering or graph-model refactors, add fixtures that prove graph
IDs, connector IDs, geometry, diagnostics, and metadata either stay stable or are
migrated deliberately in `ide/web`.
