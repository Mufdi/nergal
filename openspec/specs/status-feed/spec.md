# status-feed Specification

## Purpose

Surface active incidents of the model providers behind the wrapped agents (Anthropic for Claude Code, OpenAI for Codex) without the user leaving Nergal. OpenCode and Pi are model-agnostic and have no dedicated provider status page.

## Requirements

### Requirement: Backend polls provider Statuspage APIs

A background task (`feeds::run_status_feed`, spawned once at Tauri setup) SHALL poll `https://status.claude.com/api/v2/status.json` and `https://status.openai.com/api/v2/status.json` (both Statuspage v2) every 5 minutes and emit `status:providers` with the full provider list on **every** poll — not only on change — because Tauri events don't buffer and a webview reload would otherwise stay empty until the next status transition.

#### Scenario: Unreachable status page is not an incident

- **WHEN** a status endpoint cannot be fetched (network down, timeout)
- **THEN** that provider SHALL be reported with `indicator: "unknown"`
- **AND** the frontend SHALL NOT surface "unknown" as an incident

### Requirement: Frontend surfaces incidents in the status bar

The frontend SHALL mirror the feed into `providerStatusAtom`; providers whose indicator is neither `"none"` nor `"unknown"` are active incidents. The status bar SHALL render one chip per active incident (yellow for `minor`, red otherwise) that opens the provider's status page in the browser panel, and a toast SHALL fire only on an observed transition into (or between) incident states.

#### Scenario: All systems operational shows nothing

- **WHEN** every provider reports `indicator: "none"`
- **THEN** no incident chip SHALL render in the status bar

#### Scenario: Incident chip appears and links to the status page

- **WHEN** a provider reports `minor`, `major`, or `critical`
- **THEN** a chip with the provider name SHALL render in the status bar center group
- **AND** clicking it SHALL open the provider status page in the in-app browser panel
