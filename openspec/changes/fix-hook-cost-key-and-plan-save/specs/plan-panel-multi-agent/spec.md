# plan-panel-multi-agent

## ADDED Requirements

### Requirement: Plan approval fails if an edited plan cannot be saved

When submitting a plan decision after the user has edited the plan, the save of the edited content SHALL be persisted before the decision is sent. If the save fails, the submission SHALL return an error and the decision SHALL NOT be sent, so the user is never left believing an unsaved edit was applied while the CLI proceeds on the stale plan.

#### Scenario: save failure blocks approval

- **GIVEN** the user edited the plan and then approves it
- **WHEN** persisting the edited plan content fails (e.g. read-only directory)
- **THEN** the submission returns an error, the approval is not sent to the CLI, and the UI surfaces the failure

#### Scenario: successful save proceeds to approval

- **GIVEN** the user edited the plan and then approves it
- **WHEN** the edited content saves successfully
- **THEN** the approval is sent and the CLI re-reads the edited plan from disk
