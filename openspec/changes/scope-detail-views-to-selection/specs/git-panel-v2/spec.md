# git-panel-v2

Scopes the PR viewer's async diff/checks results to the currently displayed PR.

## ADDED Requirements

### Requirement: PR viewer results are scoped to the displayed PR

`PrViewer` SHALL apply diff and checks fetch results only if they were requested for the currently displayed `workspaceId`/`prNumber` pair; results for a previously displayed PR MUST be discarded (the shared diff cache MAY still be updated — it is keyed per PR).

#### Scenario: slow diff resolves after switching PRs

- **GIVEN** PR #12's diff fetch is in flight
- **WHEN** the user selects PR #15 in the picker before it resolves
- **THEN** #12's lines/hunks are not rendered under #15's header; #15 shows its own diff (or its loading state)

#### Scenario: stale checks are discarded

- **WHEN** a `get_pr_checks` result resolves for a PR that is no longer displayed
- **THEN** the checks state of the displayed PR is left untouched
