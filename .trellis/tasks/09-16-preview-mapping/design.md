# IMDb ingestion and mapping design

Follow the approved parent design and backend connector/mapping contracts.

Domain owns serializable Observation, item identity and supported scalar values.
Identifiers and media types remain strings. The built-in IMDb adapter parses
bounded UTF-8 CSV; it emits observations and skipped reasons, never calls a sink.
Header names, not positions, identify Const/Your Rating/Title Type; Title is an
optional display hint. IMDb's aggregate rating is never the personal rating.
Date Rated and other unsupported fields are reported skipped, not retained.

Batch validation is atomic: malformed structure, invalid supported rows or
conflicting supported duplicates reject the batch with line/code/field only.
Equivalent normalized duplicates collapse deterministically. Canonical URLs derive
from validated IMDb title IDs, never from an untrusted URL column.

Core owns the declarative evaluator; direct, scale, clamp, enum, condition and skip
are closed operations, not executable strings. Target fields follow NeoDB
capabilities. Default profile maps rating only; completion is explicit opt-in.
YAML and JSON deserialize into the same domain profile with unknown-field rejection.

Storage persists validated disabled profile versions with optimistic version
checking. Server uses session/Bearer scope guards, generated DTOs and bounded
requests. WebUI edits/imports/exports that same representation.
No source/NeoDB writes, activation, comparison jobs or raw CSV persistence.

Deliver independent verified checkpoints: observations/CSV; evaluator/YAML;
versioned profile persistence/API; shared WebUI. Keep the child active until all
acceptance criteria pass. Preview-job matching/comparison remains the next child.
