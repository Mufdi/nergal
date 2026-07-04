# issue-tracker-adapter Specification

## Purpose
TBD - created by archiving change issue-tracker-adapter. Update Purpose after archive.
## Requirements
### Requirement: Tracker integrations share one adapter contract

Tracker integrations SHALL share the mechanically-duplicated slices the spike proved
extractable with zero paper-migration leaks — a generic own-echo writeback registry
(`WritebackRegistry<F>`: provisional record before the API call, clear on failure, TTL,
parametrized over each tracker's write-field class preserving the Scalar/Additive split),
a generic keyring credential store (`CredentialStore`: keyring + atomic 0600 fallback
file, parametrized over service/account/filename, its on-disk fallback field accepting
both legacy tracker key names so no stored credential is lost on upgrade), and the
closed-out marker functions (parametrized by table name). Each tracker keeps its own
client, model, state vocabulary, poller completeness model, mirror reconcile lifecycle,
closure orchestration, AND its prompt-compose/budget framework — the compose layer's
`fit_to_budget` carries a per-tracker control-flow divergence (ClickUp has a
checklist-collapse attrition stage Linear lacks) that a shared trait would only paper over
with per-tracker overrides, the same anti-pattern the spike rejected for
poller/mirror/closure.

#### Scenario: cross-tracker fix lands once on the shared slices

- **WHEN** a defect is found in a shared slice (echo-registry logic, keyring fallback,
  closed-out marker)
- **THEN** the fix is made once in the shared module and every tracker inherits it

#### Scenario: a third tracker reuses the shared slices, not the whole stack

- **WHEN** a new tracker (e.g. GitHub Issues) is integrated
- **THEN** it reuses the shared writeback registry / credential store / closed-out marker,
  and still writes its own client, model, poller, mirror reconcile, and prompt-compose
  (those layers do not converge across trackers — confirmed by the GitHub-Issues paper
  sanity-check)

#### Scenario: behavior is preserved across the extraction

- **WHEN** ClickUp and Linear are migrated onto the shared slices
- **THEN** no tracker behavior, DB schema, or public command surface changes — the shared
  code is behavior-identical to the per-tracker code it replaces, verified by the existing
  per-tracker tests consolidating onto it

