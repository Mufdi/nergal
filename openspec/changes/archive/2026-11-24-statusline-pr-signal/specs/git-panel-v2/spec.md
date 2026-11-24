## ADDED Requirements

### Requirement: PR status refreshes on the status line signal
A session's PR signature SHALL be the open GitHub PR number from the latest `agent:status-update` `github_pr`, or `none` when `github_pr` is null or describes a GitLab merge request (`kind = "mr"`). Each cached PR status SHALL record the signature current when its fetch started. While the git panel is mounted for a session, the frontend SHALL call `get_pr_status` when no PR status is cached, when the current signature differs from the cached one, or when the cache is older than 60 seconds; it SHALL check on mount, when the session's signature changes, and every 10 seconds against that 60-second age. `get_pr_status` is the authority for the panel: a response with a PR SHALL be cached even when its number differs from the signature. A failed call, or an empty response while the signature names a PR, SHALL keep the previously cached PR status, record the current signature, and wait for the 60-second age before the next try. A check SHALL NOT start a fetch while one for the same session is in flight; a refresh after a PR merge SHALL start a new fetch anyway and supersede the one in flight. A response SHALL be dropped when a later fetch for the same session already started, or when the session no longer exists. The PR status SHALL NOT be part of the 5-second git poll. A successful Ship SHALL cache the PR it returned at once, without another `gh` call, unless the returned PR number is 0 (unknown), in which case it SHALL start a forced refresh. A PR merge from the PR viewer SHALL refresh the owning session's PR status at once when the session survives the merge cleanup (a successful cleanup deletes the session).

#### Scenario: A new pull request appears
- **GIVEN** the cached PR status of session S was fetched with signature `none` 10 seconds ago
- **WHEN** an `agent:status-update` for S carries `github_pr.number = 12`
- **THEN** the panel SHALL call `get_pr_status` for S once

#### Scenario: The pull request merges or closes
- **GIVEN** the cached PR status of S was fetched with signature PR 12
- **WHEN** an update for S carries `github_pr: null`
- **THEN** the panel SHALL call `get_pr_status` for S once
- **AND** the panel SHALL show PR 12 with state MERGED or CLOSED, as `gh pr view` reports it

#### Scenario: Repeated updates without a change
- **GIVEN** the cached PR status of S was fetched with signature PR 12, 10 seconds ago
- **WHEN** ten updates for S carry PR 12 with any review state
- **THEN** none of them SHALL call `get_pr_status`

#### Scenario: A change while the panel showed another session
- **GIVEN** the cached PR status of S has signature `none` and an update set S's signature to PR 12 while the panel showed session T
- **WHEN** the user switches the panel to S
- **THEN** the panel SHALL call `get_pr_status` for S on mount

#### Scenario: Ship caches its PR without waiting for the status line
- **WHEN** `git_ship` for S succeeds and returns PR 13 while S's signature is still PR 12
- **THEN** the cache SHALL show PR 13 right away, without a `get_pr_status` call

#### Scenario: The worktree branch differs from Claude Code's branch
- **GIVEN** S's signature is PR 12 and `get_pr_status` for S returns PR 30 (the branch Nergal tracks for S)
- **WHEN** the panel refreshes S
- **THEN** the cache SHALL show PR 30
- **AND** the panel SHALL NOT fetch again for S until the cached status is 60 seconds old or the signature changes

#### Scenario: No overlapping fetches
- **GIVEN** a `get_pr_status` for S is in flight
- **WHEN** a check for S runs
- **THEN** it SHALL NOT start another fetch

#### Scenario: A failed fetch keeps the pill and retries
- **GIVEN** the cached PR status of S shows PR 12
- **WHEN** a refresh for S fails, or returns no PR while S's signature is PR 12 (`gh` failures reach the frontend as an empty result)
- **THEN** the cache SHALL still show PR 12
- **AND** no new fetch SHALL start for S until the cached status is 60 seconds old or the signature changes

#### Scenario: Out-of-order responses
- **GIVEN** a fallback fetch for S started, then a Ship refresh for S started
- **WHEN** the Ship response (PR 12) arrives before the fallback response (no PR)
- **THEN** the cache SHALL keep PR 12

#### Scenario: Fallback poll
- **GIVEN** the git panel is mounted for S and no status line update arrives
- **WHEN** 70 seconds pass since the last fetch
- **THEN** the panel SHALL have called `get_pr_status` for S

#### Scenario: GitLab merge requests do not drive refreshes
- **WHEN** updates for S carry `github_pr` with `kind = "mr"` and changing numbers
- **THEN** the signature SHALL stay `none` and SHALL NOT trigger a refresh
