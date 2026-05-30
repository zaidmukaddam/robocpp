# Diagnostic Compatibility Policy

Diagnostics are part of the review and IDE integration surface. They should be
stable enough for corpus review, language-service clients, and release evidence.

## Stable By Default

- Diagnostic severity.
- Diagnostic code family.
- Whether invalid input is rejected.
- The primary variable, POU, type, task, access path, or XML element named by the
  diagnostic.
- Snapshot substrings in `validation/corpus/rejected/*.diag`.

## Allowed To Change With Review

- Human wording can change when the new message is clearer and all `.diag`
  sidecars are updated in the same change.
- Spans can change when parser recovery or source mapping improves.
- Additional diagnostics can be added if the original rejection remains present
  and no invalid recovered IR becomes executable.

## Not Allowed Without A Compatibility Note

- Downgrading an error to a warning.
- Removing the only diagnostic that rejects an invalid fixture.
- Letting parser recovery produce executable behavior for an unsupported or
  invalid construct.
- Changing JSON diagnostic code names without a release-note migration note.

Run:

```sh
cargo run -p xtask -- validate-corpus
```

This checks rejected syntax, semantic, PLCopen XML, limit, configuration, call,
access-path, IL, SFC, LD, and FBD diagnostics against stable sidecar
expectations.
