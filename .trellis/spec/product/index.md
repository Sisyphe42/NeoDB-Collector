# Product Specification

This layer owns product facts that must remain available before and across implementation tasks.

## Source of Truth

- [`collector-spec.yaml`](./collector-spec.yaml) is canonical for resolved/open decisions, platform capabilities, field scope, deployment, authentication, connector manifests, and evidence links.
- [`maintenance.md`](./maintenance.md) defines how to update product facts and derived implementation contracts.

The narrative research file under `temp/` is deliberately ignored and non-canonical. Do not cite it as implementation authority or add it to Trellis task context.

## Pre-development Checklist

- Read the relevant decisions and platform entry in `collector-spec.yaml`.
- Read the affected backend/frontend executable contracts.
- Confirm an open decision is resolved before implementing behavior that it blocks.
- Refresh `reviewed_at` and evidence when relying on time-sensitive platform behavior.

## Quality Check

- Product YAML parses successfully and has unique decision IDs.
- A decision ID cannot be both resolved and open.
- Derived Markdown contracts do not contradict the product YAML.
- Temporary research is not referenced by code, tests, or task manifests.
