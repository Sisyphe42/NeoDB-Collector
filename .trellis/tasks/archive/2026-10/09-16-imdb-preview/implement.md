# Execution order

1. Implement and verify preview-auth (local sessions, pairing, credential adapter, OAuth, CLI/UI).
2. Implement and verify preview-mapping (IMDb CSV, observations, safe profiles, UI editor).
3. Implement and verify preview-jobs (native catalog-fetch resolver, read-only user-state comparison, persistence, recovery, retention, UI/CLI).
4. Full parent integration: prove no source/personal-state writes, bounded catalog fetching, no raw persistence, cross-layer contracts and all CLI/UI paths.
5. Run pnpm verify; update executable specs and acceptance evidence. Make verified ordinary-code checkpoint commits per workflow; request review only for direction changes, unclear scope or missing verification. Archive only completed children/parent. No push or live OAuth without user interaction.

Rollback uses reviewed follow-up commits, never destructive worktree reset. Each migration stays forward-only; older binaries reject newer schema. Do not archive this parent until all three child acceptances pass.
