# Evidence: linear-state-order-from-data

- Mirror check (read-only, `~/.config/nergal/nergal.db`): both teams store Duplicate with type `duplicate`; Shipped `completed` 954.09; In Review `started` 1002.
- Linear changelog review (Sept–Nov 2026, linear.app/changelog): nothing changes workflow-state ordering; `retiredAt` on teams is an MCP field, not confirmed in GraphQL. No panel change taken from it.
- iprev: 2 rounds, sonnet. R1 REVISE, R2 APPROVED.
- Red: `linearStateOrder.test.ts` rewritten and `linearFold.test.ts` Pending case replaced by a separate test-writer; 6 failing before Green; locked.
- Green: `linearStateOrder.test.ts` + `linearFold.test.ts` pass; full gates run with the batch.
- Final gates (batch): `tsc` clean; `pnpm test` 458 passed; `work-gates.sh`: 3 locks unchanged, gitleaks clean.
- Walk 2.2 passed on the 21:39 `.deb` (user, 2026-11-19).
