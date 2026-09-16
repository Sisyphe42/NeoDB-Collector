# NeoDB Collector agent entrypoint

This file is the always-loaded repository entrypoint. Keep it short: workflow lives in `.trellis/workflow.md`, product facts live in `.trellis/spec/product/`, executable engineering contracts live in the other `.trellis/spec/` layers, and task-specific requirements live in `.trellis/tasks/`.

## Read order and authority

1. Follow the current user request and higher-level instructions.
2. Read the current Trellis state with `python ./.trellis/scripts/get_context.py`, then load the applicable phase from `.trellis/workflow.md`.
3. For product facts, decisions, connector capabilities, supported fields, authentication, and evidence, use `.trellis/spec/product/collector-spec.yaml`.
4. For implementation, read the relevant layer index and contracts under `.trellis/spec/backend/` or `.trellis/spec/frontend/`.
5. For an active task, follow its `prd.md`, optional `design.md` and `implement.md`, plus the spec/research files listed in `implement.jsonl` or `check.jsonl`.

Do not treat chat memory, archived task notes, or files under ignored `temp/` as implementation authority. If an active task conflicts with a repository-wide product invariant, update and review the canonical product spec first rather than silently overriding it.

## Trellis workflow

- `.trellis/workflow.md` is the workflow source of truth. Follow its Plan → Execute → Finish order and its task-creation and activation gates.
- Before editing implementation files, load the specific contracts referenced by the active task. Do not load or duplicate the entire spec tree without need.
- Put durable product and engineering knowledge in `.trellis/spec/`; put one-off requirements and research in the task directory; put session history in `.trellis/workspace/`.
- When implementation or debugging establishes a durable rule, update the affected spec during the same task.
- Do not rerun `trellis init` for product or convention changes. Edit project specs directly. Use `trellis update` only for Trellis runtime/template updates and review conflicts without overwriting project-owned specs.

Spec entrypoints:

- Product: `.trellis/spec/product/index.md`
- Backend: `.trellis/spec/backend/index.md`
- Frontend and extension: `.trellis/spec/frontend/index.md`
- Cross-layer checklists: `.trellis/spec/guides/index.md`

## Non-negotiable product boundaries

- Current sources are read-only; NeoDB is the only write target. Reading NeoDB for comparison is allowed, but no source-platform mutation is authorized.
- Only NeoDB-supported fields are in scope. Unsupported source data is reported as skipped and is not retained as a general media-history warehouse.
- Connectors emit validated observations; identity resolution, mapping, planning, policy, and execution remain separate. A connector must not call another connector or NeoDB directly.
- Platform IDs and media types are open namespaced strings, not closed Core enums. MVP executes built-in connectors only while preserving the reviewed manifest boundary for future extensions.
- Mapping is declarative and cannot execute connector-supplied JavaScript, shell, SQL, or equivalent code. New connectors remain write/schedule disabled until a successful dry-run is explicitly approved.
- Exactly one configurable NeoDB endpoint/account is supported unless the product spec is deliberately revised.
- SQLite stores operational state only. Raw source API/page payloads, cookies, passwords, and plaintext tokens must never be persisted.
- Secrets use the OS credential store or explicit container secrets. Pairing uses a short-lived single-use URI-fragment key, then a revocable per-client Bearer token; long-lived secrets never go in URI paths or queries.
- Browser-session access uses the first-party extension and the user's existing session. Do not request the browser `cookies` permission, export/import raw cookies, store source passwords, bypass access controls, or automate CAPTCHA/MFA.
- Desktop, CLI WebUI, Docker, browser extension, and local plugins share one versioned Core API. Windows, macOS, and Linux are equal desktop targets; platform-specific behavior stays behind adapters.
- Agent assistance is optional: configuration validation, dry-run, review, and scheduled synchronization must remain deterministic and fully usable without a model or external agent service.
- Do not bundle or reimplement a general workflow engine. A future first-party n8n node is a thin client of the versioned Core API and is never loaded as executable code by Core.
- Keep the approved Monorepo dependency direction in `.trellis/spec/backend/repository-layout.md`: external clients and integrations consume generated contracts or the versioned Core API, never Core internals.

## Change maintenance

- Product/platform/API changes: verify with primary evidence, update `.trellis/spec/product/collector-spec.yaml`, refresh `reviewed_at` and evidence when time-sensitive, then update affected executable contracts and tests.
- Engineering convention changes: update the owning `.trellis/spec/` file with real source/test paths once code exists; remove obsolete rules instead of retaining parallel conventions.
- Workflow or Trellis integration changes: update `.trellis/workflow.md` first, then keep affected `.codex/`, `.agents/`, hooks, skills, and runtime-agent files aligned.
- Never make code, tests, or task context depend on ignored `temp/` files.

## Tooling, validation, and delivery

- On Windows, prefer global package managers in this order: `scoop = npm = pip > choco > winget > vcpkg > powershell`.
- Prefer `rg` / `rg --files` for search and `apply_patch` for authored edits. Preserve unrelated user changes.
- Before handoff, run the relevant tests and contract checks plus `git diff --check`. Do not claim completion when a required check is failing or was not run.
- Commits should be scoped and accurately typed. Creating or merging pull requests and other high-risk Git operations require explicit user approval.
- Commit each coherent ordinary-code checkpoint after its relevant tests, contract checks, and `git diff --check` pass. No additional user confirmation is needed when all files are recognized and in scope and product/workflow direction is unchanged; direction-changing documentation, unclear ownership, missing verification, and high-risk Git operations still require review or explicit approval.
- GitHub release draft content defaults to `Full Changelog: <tag compare>`.
