# session-summary

## ADDED Requirements

### Requirement: Cost is keyed by the Nergal session id

The per-session cost record written on a Stop event SHALL be keyed by the Nergal session id (the DB's `sessions.id` key), consistent with every other Stop-handler write, so cost is attributable by the same id used everywhere else and survives session resume.

#### Scenario: cost stored under the Nergal id

- **WHEN** a Stop event carrying both the Claude-Code-internal id and the Nergal session id is processed
- **THEN** the cost row is stored under the Nergal session id and is readable by it

#### Scenario: resumed session keeps one cost lineage

- **GIVEN** a session that is resumed (receiving a new Claude-Code-internal id)
- **WHEN** cost is upserted on Stop
- **THEN** it accumulates under the stable Nergal session id rather than fragmenting across internal ids
