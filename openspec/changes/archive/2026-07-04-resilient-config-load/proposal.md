## Why

A malformed `config.json` silently destroys the user's entire configuration. `Config::load` (`src-tauri/src/config.rs:433-440`) does `serde_json::from_str(&contents).unwrap_or_default()` — any deserialize error (truncated write from a crash, a hand-edit typo, a partial disk) is swallowed and replaced with `Self::default()`. The next `config.save()` (`config.rs:443-451`, triggered by almost any settings change) then atomically overwrites the corrupt-but-recoverable file with defaults, permanently losing agent overrides, keymap, themes, and tracker tuning.

## What Changes

- **Distinguish "file missing" from "file unparseable"** in `Config::load`: missing → defaults (unchanged); unparseable → log the parse error (`tracing::error!` with the serde message), **back up the original file** to `config.json.corrupt` (sibling path, overwrite any previous backup) before returning defaults.
- The backup happens at load time (not save time), so the original bytes survive even if the app saves later; the user (or support) can recover fields by hand.
- No behavior change for the happy path or for a missing file.

## Capabilities

### New Capabilities

- `config-persistence`: a malformed config file is preserved (backed up) and surfaced in logs, never silently replaced.

### Modified Capabilities

_None._

## Impact

- **`src-tauri/src/config.rs`**: `Config::load` (~433-440) restructures the `match` — `Ok(contents)` branch matches on `serde_json::from_str` explicitly; error branch writes the backup (best-effort, `let _ =`-free: log a second error if the backup itself fails) and falls back to defaults.
- **Tests**: unit tests with a tempdir-pointed config path — valid file loads; missing file → defaults, no backup; corrupt file → defaults, `config.json.corrupt` contains the original bytes, original file untouched until next save.
- **Risk**: MEDIUM severity averted (permanent config loss) for LOW change complexity; the only subtlety is not clobbering an older `.corrupt` backup that the user still needs — acceptable: the newest corruption is the one being debugged (documented in the log line).
- **Out of scope**: schema-migrating partially-valid configs (field-level recovery); a UI notification for the corruption (log-only for now — could be a follow-up toast).
