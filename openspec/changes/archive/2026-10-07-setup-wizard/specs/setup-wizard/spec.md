## ADDED Requirements

### Requirement: Hooks status probe
The system SHALL expose a read-only backend command that reports the hooks setup state. The response MUST contain:
- `registered` (bool): `true` exactly when running the hooks setup would leave the settings unchanged. That means every Nergal hook present with its current matcher, no obsolete Nergal hook left, and a `statusLine` key present.
- `settings_path` (string): absolute path of `~/.claude/settings.json`.
- `binary_on_path` (bool): on Unix, whether a `nergal` binary resolves on the app's `PATH` outside the running AppImage mount (`$APPDIR`). Unix hooks call bare `nergal hook …`, so they only run when this is `true`. On Windows it is always `true`, because the hooks pin the absolute executable path.

The probe MUST NOT create, modify or delete any file or directory.

#### Scenario: Settings file absent
- **GIVEN** neither `~/.claude/` nor `~/.claude/settings.json` exists
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `false` and `settings_path` is `"<home>/.claude/settings.json"`
- **AND** neither the directory nor the file exists afterward

#### Scenario: All hooks present
- **GIVEN** the settings file is the output of a completed hooks setup
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `true`

#### Scenario: One hook missing
- **GIVEN** the settings file is the output of a completed hooks setup with the `SessionStart` entry removed
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `false`

#### Scenario: Outdated matcher
- **GIVEN** the settings file is the output of a completed hooks setup with the `nergal hook send tool-done` entry's matcher set to `"Write|Edit"`
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `false`

#### Scenario: Obsolete hook present
- **GIVEN** the settings file has every current hook plus one entry listed as obsolete by the setup
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `false`

#### Scenario: statusLine key absent
- **GIVEN** every Nergal hook is present and the root has no `statusLine` key
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `false`

#### Scenario: User-owned statusLine kept
- **GIVEN** every Nergal hook is present and `statusLine` holds a user command that is not Nergal's
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `true`

#### Scenario: Wrapper-form hooks count as present
- **GIVEN** every Nergal hook is present in the `nergal-conditional.sh` wrapper form
- **WHEN** the frontend invokes the probe
- **THEN** `registered` is `true`

#### Scenario: Unreadable settings file
- **GIVEN** the settings file contains invalid JSON
- **WHEN** the frontend invokes the probe
- **THEN** the command returns an error and the file bytes are unchanged

#### Scenario: Malformed Nergal hook event value
- **GIVEN** the settings file has `"hooks": { "SessionStart": {} }`
- **WHEN** the frontend invokes the probe or the hooks setup runs
- **THEN** both return an error naming `SessionStart`, and the file bytes are unchanged

#### Scenario: Malformed unrelated hook event value
- **GIVEN** the settings file has `"hooks": { "MyEvent": null }` and no Nergal hooks
- **WHEN** the hooks setup runs
- **THEN** it succeeds, every Nergal hook is added, and `hooks.MyEvent` is still `null`

#### Scenario: Binary inside the AppImage mount does not count
- **GIVEN** `APPDIR` is `/tmp/.mount_nergalX` and the only `nergal` on `PATH` is `/tmp/.mount_nergalX/usr/bin/nergal`
- **WHEN** the probe runs on Unix
- **THEN** `binary_on_path` is `false`

#### Scenario: Installed binary counts under an AppImage
- **GIVEN** `APPDIR` is `/tmp/.mount_nergalX` and `nergal` on `PATH` resolves to `/usr/bin/nergal`
- **WHEN** the probe runs on Unix
- **THEN** `binary_on_path` is `true`

### Requirement: Hooks setup dialog at launch
At app launch the system SHALL invoke the hooks status probe. When the probe returns `registered: false`, the system SHALL open a dialog that names `settings_path` and offers a "Configure" action and a "Skip" action. When the probe returns `registered: true` or an error, the dialog MUST NOT open. Pressing Enter while focus is on the dialog itself SHALL activate "Configure" when it is enabled. Pressing Enter while focus is on a button SHALL activate that button only. Pressing Escape SHALL activate "Skip".

#### Scenario: Fresh install opens the dialog
- **GIVEN** the probe returns `{ registered: false, settings_path: "/home/u/.claude/settings.json", binary_on_path: true }`
- **WHEN** the app mounts
- **THEN** the dialog is open and shows the text `/home/u/.claude/settings.json`

#### Scenario: Configured install sees no dialog
- **GIVEN** the probe returns `registered: true`
- **WHEN** the app mounts
- **THEN** the dialog is not rendered

#### Scenario: Probe error sees no dialog
- **GIVEN** the probe rejects with an error
- **WHEN** the app mounts
- **THEN** the dialog is not rendered

#### Scenario: Enter configures
- **GIVEN** the dialog is open, "Configure" is enabled and focus is on the dialog
- **WHEN** the user presses Enter
- **THEN** `setup_hooks` is invoked once and "Skip" is not activated

#### Scenario: Enter on a focused Skip skips
- **GIVEN** the dialog is open and the user moved focus to "Skip" with Tab
- **WHEN** the user presses Enter
- **THEN** the dialog closes and `setup_hooks` has been invoked 0 times

### Requirement: Binary must be on PATH before configuring
When the probe returns `binary_on_path: false`, the dialog SHALL disable "Configure" and show that `nergal` is not on `PATH`. The system MUST NOT write hooks that cannot run.

#### Scenario: AppImage without nergal on PATH
- **GIVEN** the probe returns `{ registered: false, binary_on_path: false }`
- **WHEN** the dialog opens
- **THEN** "Configure" is disabled, the dialog shows that `nergal` is not on `PATH`, and pressing Enter invokes `setup_hooks` 0 times

### Requirement: Consented hooks configuration
The system MUST NOT write `~/.claude/settings.json` from the GUI without the user activating "Configure". Activating "Configure" SHALL invoke the existing `setup_hooks` command. On success the dialog closes. On failure the dialog stays open and shows the rejection message.

#### Scenario: No write before consent
- **GIVEN** the dialog is open
- **WHEN** no action is activated
- **THEN** `setup_hooks` has been invoked 0 times

#### Scenario: Configure succeeds
- **GIVEN** the dialog is open
- **WHEN** the user activates "Configure" and `setup_hooks` resolves
- **THEN** `setup_hooks` has been invoked exactly 1 time and the dialog is closed

#### Scenario: Configure fails
- **GIVEN** the dialog is open
- **WHEN** the user activates "Configure" and `setup_hooks` rejects with a message
- **THEN** the dialog stays open and shows that message

#### Scenario: Setup is idempotent
- **GIVEN** the settings file is the output of a completed hooks setup
- **WHEN** `setup_hooks` runs again
- **THEN** the settings file content is unchanged and no hook entry is duplicated

### Requirement: Skip lasts one launch
Activating "Skip" SHALL close the dialog without invoking `setup_hooks`. The system MUST NOT persist the skip. The next app launch SHALL probe again and open the dialog if the hooks are still not registered.

#### Scenario: Skip closes without writing
- **GIVEN** the dialog is open
- **WHEN** the user activates "Skip"
- **THEN** the dialog is closed and `setup_hooks` has been invoked 0 times

#### Scenario: Next launch asks again
- **GIVEN** the user skipped on the previous launch and the hooks are still not registered
- **WHEN** the app mounts again
- **THEN** the dialog is open
