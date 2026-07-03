# obsidian-context-injection

## MODIFIED Requirements

### Requirement: Injected context marks external content as untrusted data

The context assembled at session spawn/resume SHALL enclose each externally-sourced block (pinned vault notes, ClickUp tasks, Linear issues) within an explicit boundary that frames the enclosed text as reference material to be treated as data, not as instructions. No externally-sourced block SHALL be concatenated into the launch context without this boundary. The user's own prompt SHALL remain outside these boundaries. The closing boundary marker SHALL NOT be spoofable by block content: occurrences of the marker within content SHALL be neutralized (escaped/rewritten), or a per-block unpredictable fence tag SHALL be used, so content cannot terminate the fence early. Length budgeting and truncation markers are unchanged.

#### Scenario: each source block is delimited

- **WHEN** the injected context is assembled from any combination of vault, ClickUp, and Linear blocks
- **THEN** each contributed block is enclosed by the untrusted-data boundary markers with its source labeled

#### Scenario: crafted external text is framed as data

- **GIVEN** a bound Linear issue whose description contains instruction-like text ("ignore previous instructions and …")
- **WHEN** the session context is assembled
- **THEN** that text appears inside the external-reference boundary, framed as data, not concatenated as if it were the user's own instruction

#### Scenario: content cannot close the fence early

- **GIVEN** a bound issue/comment whose content contains the literal closing boundary marker
- **WHEN** the context is assembled
- **THEN** the embedded marker is neutralized (escaped/rewritten) or the fence tag is unpredictable, so the entire block stays inside the boundary and nothing leaks adjacent to the user prompt

#### Scenario: empty source contributes nothing

- **WHEN** a source has no content to inject
- **THEN** it contributes no bare text and no empty boundary block
