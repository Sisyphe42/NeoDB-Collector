# Implementation checkpoints

1. Domain Observation/scalar/item contracts and built-in IMDb CSV parser. Bound
   input to 10 MiB/10,000 data rows; test UTF-8/BOM, named columns, quoting/newlines,
   line diagnostics, media skips, rating validation, duplicate conflicts and caps.
2. Canonical declarative MappingProfile and Core evaluator. Add operation/target
   validation, default rating-only profile, completion opt-in and YAML/JSON golden
   round-trips. No Date Rated to watched-date inference.
3. Disabled profile version persistence and authenticated validation/save API.
   Migration and optimistic version tests; regenerate all wire contracts.
4. WebUI profile editor/YAML import/export through the shared generated client.
   Show validation errors, preserve disabled status and test round-trip behavior.
5. Run scoped tests and full pnpm verify at coherent checkpoints, update executable
   specs and commit automatically. Do not archive until all child criteria pass.
