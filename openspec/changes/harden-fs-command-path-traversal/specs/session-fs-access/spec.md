# session-fs-access

Confines every session-scoped filesystem command to the session's resolved cwd (or the resolved openspec directory), so a caller-supplied path cannot escape the intended base via absolute paths, `..` traversal, or symlink indirection.

## ADDED Requirements

### Requirement: Path confinement for session filesystem commands

Every Tauri command that resolves a caller-supplied path against a session base (`read_file_content`, `write_file_content`, `list_directory`, `save_conflict_resolution`, `get_file_conflict_versions`, `read_openspec_artifact`, `write_openspec_artifact`) SHALL validate the resolved path stays within the base before any filesystem read or write, and SHALL return an error without touching the filesystem when it does not. For `read_openspec_artifact`/`write_openspec_artifact`, BOTH caller-controlled inputs (`change_name` and `artifact_path`) SHALL be validated against the openspec base.

The validation SHALL:
- reject any input containing a parent-directory (`..`) component,
- resolve an absolute input as the target directly (not joined onto the base) and accept it ONLY if it canonicalizes inside the base,
- reject any resolved target whose canonical form does not start with the canonical base (catches symlink escapes and absolute escapes).

#### Scenario: absolute path outside the base is rejected

- **WHEN** `read_file_content` is called with `path = "/etc/passwd"` for a session whose cwd is `/home/u/proj`
- **THEN** the command returns an error and performs no read

#### Scenario: absolute path inside the base is accepted

- **GIVEN** the editor opens a tab whose stored `filePath` is the absolute `/home/u/proj/src/main.rs`
- **WHEN** `read_file_content` is called with that absolute path for a session whose cwd is `/home/u/proj`
- **THEN** the path canonicalizes inside the base and the read succeeds (the editor flow is unbroken)

#### Scenario: openspec change_name traversal is rejected

- **WHEN** `read_openspec_artifact` is called with `change_name = "../../.."` (or an `artifact_path` with `..`)
- **THEN** the command returns an error and reads nothing

#### Scenario: parent-traversal is rejected

- **WHEN** `write_file_content` is called with `path = "../../secret.txt"` for a session cwd `/home/u/proj`
- **THEN** the command returns an error and writes nothing

#### Scenario: symlink escape is rejected

- **GIVEN** `proj/link` is a symlink pointing to `/home/u/.ssh`
- **WHEN** `read_file_content` is called with `path = "link/id_rsa"`
- **THEN** the canonical target does not start with the canonical cwd and the command returns an error

#### Scenario: legitimate nested path is accepted

- **WHEN** `list_directory` is called with `path = "src/components"` for a session cwd `/home/u/proj`
- **THEN** the command resolves `/home/u/proj/src/components` and returns its entries

#### Scenario: new file inside the base is accepted

- **GIVEN** `proj/notes/new.md` does not yet exist
- **WHEN** `write_file_content` is called with `path = "notes/new.md"`
- **THEN** the guard walks up to the nearest existing ancestor, validates it stays within the base, and the write succeeds

#### Scenario: new nested directory under the base is accepted

- **GIVEN** `openspec/changes/x/specs/cap/` does not yet exist
- **WHEN** `write_openspec_artifact` targets `specs/cap/spec.md` (which `create_dir_all`s the subtree)
- **THEN** the guard walks up to the nearest existing ancestor inside the openspec base and the write succeeds
