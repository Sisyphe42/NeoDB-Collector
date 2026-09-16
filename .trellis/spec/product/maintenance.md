# Maintaining Product and Engineering Specs

## Product or Platform Changes

When a platform API, supported field, authorization rule, product decision, or connector capability changes:

1. Verify the current behavior with primary evidence.
2. Update `.trellis/spec/product/collector-spec.yaml`, including `reviewed_at` and evidence links when time-sensitive.
3. Update each affected executable contract under `.trellis/spec/backend/` or `.trellis/spec/frontend/`.
4. Update `AGENTS.md` only when the change is a repository-wide invariant.
5. Update or add task-specific fixtures and tests during implementation.
6. Validate YAML, spec links, decision-ID uniqueness, and contradictions.

Do not rerun `trellis init`. Initialization creates the workflow scaffold; it does not infer new platform facts or rewrite project conventions correctly.

## Code Convention Changes

When implementation establishes or changes a repeatable convention:

1. Update the relevant `.trellis/spec/` contract with real source/test paths and examples.
2. Update task `implement.jsonl` and `check.jsonl` references when the task must load the changed contract.
3. Remove obsolete rules instead of leaving parallel old/new conventions.

No generator run is required.

## Trellis Runtime Updates

When the Trellis tool itself is upgraded, use `trellis update`, not `trellis init`. Trellis compares managed templates using `.trellis/.template-hashes.json`; locally modified files may be preserved, presented as conflicts, or emitted as `.new` sidecars. Review the diff and keep project-owned `.trellis/spec/**` content.

Use `trellis upgrade` only to upgrade the globally installed CLI package; follow it with `trellis update` inside this repository when the new runtime files should be applied.
