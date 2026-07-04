## MODIFIED Requirements

### Requirement: Tracker integrations share one adapter contract

Tracker integrations SHALL share the mechanically-duplicated slices the spike proved
extractable with zero paper-migration leaks — a generic own-echo writeback registry
(`WritebackRegistry<F>`: provisional record before the API call, clear on failure, TTL,
parametrized over each tracker's write-field class preserving the Scalar/Additive split),
a generic keyring credential store (`CredentialStore`: keyring + atomic 0600 fallback
file, parametrized over service/account/filename, its on-disk fallback field accepting
both legacy tracker key names so no stored credential is lost on upgrade), the
closed-out marker functions (parametrized by table name), and the pure
description-truncation helper (`head_tail_truncate` + its `DESCRIPTION_TRUNC_MARKER`,
byte-identical across trackers, type-agnostic). Each tracker keeps its own
client, model, state vocabulary, poller completeness model, mirror reconcile lifecycle,
closure orchestration, AND its prompt-compose/budget framework — the compose layer's
`fit_to_budget` *attrition orchestration* carries a per-tracker control-flow divergence
(ClickUp has a checklist-collapse attrition stage Linear lacks, over different item types
and field names) that a shared trait would only paper over with per-tracker overrides, the
same anti-pattern the spike rejected for poller/mirror/closure. Only the *pure helper that
orchestration calls* (`head_tail_truncate`) is shared; the orchestration, `render`, and the
per-tracker fence-sentinel neutralization stay per-adapter.

#### Scenario: cross-tracker fix lands once on the shared slices

- **WHEN** a defect is found in a shared slice (echo-registry logic, keyring fallback,
  closed-out marker, description truncation)
- **THEN** the fix is made once in the shared module and every tracker inherits it

#### Scenario: a third tracker reuses the shared slices, not the whole stack

- **WHEN** a new tracker (e.g. GitHub Issues) is integrated
- **THEN** it reuses the shared writeback registry / credential store / closed-out marker /
  truncation helper, and still writes its own client, model, poller, mirror reconcile, and
  prompt-compose orchestration (those layers do not converge across trackers — confirmed by
  the GitHub-Issues paper sanity-check)

#### Scenario: behavior is preserved across the extraction

- **WHEN** ClickUp and Linear are migrated onto the shared slices
- **THEN** no tracker behavior, DB schema, or public command surface changes — the shared
  code is behavior-identical to the per-tracker code it replaces, verified by the existing
  per-tracker tests consolidating onto it
