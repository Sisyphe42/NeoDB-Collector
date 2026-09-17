# Milestone progress

2026-09-17: preview-auth implementation is complete and verified locally.
Local WebUI/session/CSRF, pairing/revocation, restrictive read-only NeoDB OAuth,
OS credential adapters, CLI pair/status and generated contracts are implemented.
No real-account or native-keychain test is claimed.

Remaining children in order: preview-mapping, then preview-jobs. Review their
existing planning artifacts before activation. Parent acceptance is not complete:
the internal IMDb CSV adapter and pure mapping evaluator/YAML/JSON codec now exist,
and immutable disabled profile persistence plus authenticated profile API are
implemented. Importer API, profile editor and preview execution remain incomplete.

2026-09-17: user approved native catalog fetching after upstream API/web equivalence
verification (2732542). Preview may create catalog entries, never personal state.
Implementation of fetching remains in preview-jobs after preview-mapping completes.
