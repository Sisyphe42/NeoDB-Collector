# Runtime library evidence

Reviewed 2026-09-16 during implementation. Versions actually installed are in the lockfiles, not this note.

- Utoipa route/schema derivation and OpenAPI 3.1: https://docs.rs/utoipa/latest/utoipa/
- Schemars derive and JSON Schema: https://docs.rs/schemars/latest/schemars/
- SQLx embedded migrations: https://docs.rs/sqlx/latest/sqlx/migrate/struct.Migrator.html
- openapi-typescript generation: https://openapi-ts.dev/introduction
- npm registry metadata (`npm view`) confirmed current React/Vite/Vitest/openapi-fetch/AJV versions. TypeScript latest was outside openapi-typescript/typescript-eslint peer ranges; pinned 5.9.3 with a warning-free install.

Implementation findings: Schemars emits `uint32` as a format annotation without an upper bound. DTOs now carry explicit 0..4294967295 ranges in both generated schemas; AJV recognizes the annotation and enforces numeric constraints. pnpm can expose either a native executable or JS entrypoint through npm_execpath, so the verification runner supports both.

Local environment: Scoop Rust binaries include cargo-fmt and cargo-clippy but their directory was not on PATH. Added that directory only to the verification process environment. Global Cargo cache had incomplete extracted byteorder sources; used ignored `temp/cargo-home` for a fresh cache. No global package/cache repair performed.
