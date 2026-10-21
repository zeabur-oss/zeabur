# zeabur-fixtures

Loads Zeabur's language-neutral test cases and compares results with them. Only ever a
dev-dependency.

A case is a directory holding `case.yaml`, an optional `input.yaml` with Kubernetes objects and
`expected.json`. Each crate keeps the cases it is tested with in its own `fixtures/<area>/`, so
a packaged crate carries them; `load!("<area>")` reads them. Objects stay
[`serde_json::Value`]s here: the crate under test decides how to type them. The format is
described in the repository's `fixtures/README.md`.

## Place in the workspace

Level 0: depends on no other workspace crate.
