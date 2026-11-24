## ADDED Requirements

### Requirement: Statusline feeder forwards the pull request
The `nergal hook statusline` feeder SHALL map CC's status line `pr` object into the `AgentStatus` socket payload as `github_pr` with the optional fields `number` (u64), `url` (string), `review_state` (string) and `kind` (string). A missing or non-object `pr` SHALL map to `github_pr: null` without failing the payload. The backend SHALL pass `github_pr` unchanged in the `agent:status-update` event.

#### Scenario: Open pull request
- **WHEN** the status line stdin carries `"pr": {"number": 1234, "url": "https://github.com/o/r/pull/1234", "review_state": "pending"}`
- **THEN** the payload SHALL carry `"github_pr": {"number": 1234, "url": "https://github.com/o/r/pull/1234", "review_state": "pending", "kind": null}`

#### Scenario: No pull request
- **WHEN** the status line stdin has no `pr` key
- **THEN** the payload SHALL carry `"github_pr": null`

#### Scenario: Pass-through to the frontend
- **WHEN** an `AgentStatus` socket event with `github_pr.number = 7` arrives
- **THEN** the `agent:status-update` event SHALL carry `github_pr.number = 7`
