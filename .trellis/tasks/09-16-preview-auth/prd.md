# Secure local access and NeoDB OAuth

## Goal

Deliver the security prerequisite for private import previews.

## Requirements and acceptance criteria

- [x] Single-use fragment WebUI bootstrap, HttpOnly SameSite=Strict session, CSRF checks; public health stays public.
- [x] 10-minute single-use pairing keys, transactional client creation, hashed revocable bearer tokens with fixed non-admin scopes (reserved operations are not enabled yet).
- [x] NeoDB read-scope browser OAuth bound to initiating session and instance; OS credential storage only, fail closed if unavailable.
- [x] CLI pair and authenticated status, WebUI authorize/status/client revoke controls, one shared versioned API.
- [x] Reject non-HTTPS remote endpoints, non-loopback HTTP, userinfo/fragments/query/base paths and cross-origin credential redirects.
- [x] Tests cover expiry/replay/cancellation/CSRF/revocation/redaction; real remote authorization is not performed by tests.

## Approval

User approved the complete plan with “实施计划” on 2026-09-16. Work remains read-only toward media platforms. No live account access or remote publish is authorized by this task.
