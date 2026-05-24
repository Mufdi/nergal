## MODIFIED Requirements

### Requirement: Out-of-scope operations explicitly deferred to Change B

The script SHALL NOT:
- Invoke `pnpm tauri build` or any artifact-producing command
- Create or upload to a GitHub release
- Sign artifacts
- Edit any previous release's body or banner

These responsibilities SHALL be handled by capability `release-ci-signed` — a GitHub Actions workflow triggered by the tag push that the script performs at the end of its run. Post-script, the user SHALL NOT need to run any additional local command; CI handles build, signing, release creation, and banner update.

#### Scenario: Script terminates after push with CI handoff message
- **WHEN** all script steps complete successfully
- **THEN** the script SHALL exit 0 immediately after `git push origin v<new>`
- **AND** SHALL print exactly: `CI workflow will now build, sign, and publish the release. Monitor at https://github.com/Mufdi/nergal/actions`
- **AND** SHALL NOT print the legacy "manual next steps" reminder (build / gh release create / banner edit)

#### Scenario: --no-push mode still recommends CI awareness
- **WHEN** the script runs in `--no-push` mode and completes the local commit + tag
- **THEN** the script SHALL print: `Released <tag> locally (--no-push). To push and trigger CI: git push origin main && git push origin <tag>`
- **AND** SHALL NOT push automatically
