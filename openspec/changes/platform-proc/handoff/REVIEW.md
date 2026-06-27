# Review — platform-proc

_Reviewers write here during Mode B. Empty until execution._

## iprev RUN 2026-06-26 (platform-proc)

Evaluator: Claude CLI (architect persona). Mode A plan review — plan artifacts only, no source edits.
Result: R1 7 issues [2 critical, 1 high, 2 medium, 2 low] -> REVISE; R2 0 blockers (1 new low, resolved) -> APPROVED.

Substantive revisions applied between R1 and R2:
- Threaded the second reassigned diagnostics read (read_os_pretty_name / /etc/os-release -> sysinfo long_os_version) end-to-end: proposal, design D6, spec requirement renamed + new OS-name scenario, tasks 3.1, implementation verified-facts + caller edits. This was the categorical R1 gap (platform-compile's dropped D3).
- Added a grep gate for os-release/read_os_pretty_name (the /proc grep cannot catch /etc/os-release).
- Added macOS cross-process environ-readability validation to the D2a spike (1.3b), risk register, and Open Questions; requirement may surface the limitation instead of a silent None.
- Stated the macOS manual-only coverage limit explicitly (no macOS test runner until platform-bundle-ci).
- Clarified single shipped impl (no lingering cfg(linux) /proc arm); reordered tasks so the listener crate is added after the spike verdict (1.2 sysinfo, 1.3 spike, 1.4 chosen crate).
- R2 low: relaxed spec wording from verbatim PRETTY_NAME to 'meaningful distribution string' (sysinfo constructs an equivalent, not byte-identical, string).

### Round 1 evaluator output

## Independent Review — `platform-proc` OpenSpec change

I verified the load-bearing claim against the live codebase (`src-tauri/src/updater.rs:410-448`). Findings below, most severe first.

1. **[CRITICAL] The plan accounts for only ONE of the two reads `platform-compile` reassigned to it.** `platform-compile` dropped its D3 and handed *both* `updater.rs` diagnostic reads to this change. The codebase has exactly two:
   - `updater.rs:441` — `/proc/sys/kernel/osrelease` → mapped (`platform_proc::kernel_version()`). ✅
   - `updater.rs:410-416` `read_os_pretty_name()` reading `/etc/os-release`, consumed at `updater.rs:440` as the `os` field. **Not mentioned anywhere** in proposal/design/tasks/implementation. ❌

   Every reference to `updater.rs` (Impact bullet, design "minor read", `tasks.md` 3.1, `implementation.md` "Verified codebase facts › updater.rs", caller-edits, both verification walks) names *only* the kernel read. `read_os_pretty_name` and `/etc/os-release` appear zero times. This is the exact gap the reassignment was meant to close, and the plan silently inherits `platform-compile`'s dropped responsibility without picking it up.

2. **[CRITICAL] Consequence on macOS: a real, un-caught runtime regression.** `/etc/os-release` does not exist on macOS, so `read_os_pretty_name()` returns `None` and the diagnostics `OS:` line renders `"unknown"` on every Mac — defeating the stated goal "diagnostics include a kernel/OS version on both platforms" (spec `Requirement: Kernel version for diagnostics`). Worse, it's invisible to the plan's own safety net: `tasks.md` 4.1 / `implementation.md` step 5 assert correctness by `grep -rn "/proc" src-tauri/src/`. `/etc/os-release` is not under `/proc`, so the grep gate passes green while the read survives ungated. The cleanup gate gives false confidence here. The proper fix is `sysinfo::System::long_os_version()` (or `name()`+`os_version()`), which the design already had in hand for `kernel_version()` but never applied to the OS string.

3. **[HIGH] The spec's `os`-string requirement is unspecified and untested.** The `Kernel version for diagnostics` requirement and its scenario only assert a *kernel* string. There is no requirement, scenario, task, or verification step covering the human OS name. Even if a reviewer notices issue #1 during implementation, nothing in the spec or `tasks.md` forces them to handle it, and `5.3`/`5.4` only check "diagnostics kernel string is non-`unknown`" — a Mac with `OS: unknown` would pass acceptance.

4. **[MEDIUM] `ancestor_env` on macOS depends on cross-process `environ` readability, which the risk register doesn't address.** The `sysinfo`-permission risk note only covers `cwd`/`cmd`. Reading another process's *environment* on macOS is more restricted than cwd/cmdline (it routes through `sysctl(KERN_PROCARGS2)` and is reliable only for same-uid processes, sometimes not even then). The Codex-env-recovery requirement hinges entirely on this. The D2a spike validates `netstat2` pid association but does not validate `sysinfo.environ()` across ancestors on macOS — add it to the spike or the requirement may ship dead-on-macOS, exactly the failure mode this change exists to fix for `shim.rs`.

5. **[MEDIUM] No automated macOS test coverage; the macOS path is manual-only.** `tasks.md` 2.4 tests "pure parts"; 5.2 is `cargo check --target` (compile, not execute); 5.4 is a manual walk. The cross-check CI job compiles but never runs macOS code (no macOS runner in scope). Every behavioral macOS claim (ports owner resolution, environ recovery, cwd) rests on one human walk-through. Acceptable given no macOS CI, but it should be stated as a known coverage limit, not implied as verified.

6. **[LOW] Internal inconsistency: "one impl per OS" (D4) vs "Linux impl mirrors `/proc` first, then add sysinfo impl" (`implementation.md` step 3).** D1/D4 commit to a single `sysinfo`-backed impl for all OSes; `implementation.md` step 3 reads as if a Linux `/proc` impl and a `sysinfo` impl coexist. I read this as dev *sequencing* (build/verify on `/proc`, then swap), not two shipped impls — but as written it's ambiguous and could lead to a lingering `cfg(linux)` `/proc` arm that 4.1 would then bless as "the Linux arm inside the module." Clarify that the shipped module is single-impl.

7. **[LOW] Crate is committed (1.2) before the spike that gates it (1.3 / D2a) resolves.** `netstat2` is added to `Cargo.toml` in phase 1.2, but D2a may reject it for `listeners` or `libproc`. Minor (a dep swap is cheap), but the ordering invites a committed-then-removed dependency.

### Dimension summary
- **Correctness:** Approach is sound for the `/proc` subsystems it names, but incomplete — it solves the kernel read and misses the `/etc/os-release` read it was explicitly assigned (#1).
- **Completeness:** Missing the second reassigned read end-to-end (proposal→spec→tasks→verify) (#1-3).
- **Concurrency/state:** Adequately handled — per-tick single snapshot reuse (D-risk), preserved `kill(-pgid)` ordering (D3), bounded ancestor walk. No new races.
- **Test coverage:** Pure logic preserved + gated (good); macOS execution untested (#5); `os`-string untested (#3).
- **Security:** No new injection/secret surface. `environ` read is a privacy/permission consideration on macOS, not a vuln (#4).
- **Rollback:** Per-caller revert is well-designed and genuinely reversible; partial failure is isolated. The `grep /proc` gate, however, gives false assurance for the `/etc/os-release` survivor (#2).

The fix is small in code (one `sysinfo::System::long_os_version()` call plus matching spec/task/verify lines) but the omission is categorical — the change quietly fails to absorb a responsibility formally transferred to it, and its own verification gate cannot detect the miss. That must be closed before approval.

VERDICT: REVISE

### Round 2 evaluator output

## Independent Review — `platform-proc` OpenSpec change (Round 2)

I re-checked each Round 1 finding against the revised plan.

### Disposition of Round 1 findings

1. **[was CRITICAL — second `updater.rs` read unaccounted]** — **Resolved.** The `/etc/os-release` / `read_os_pretty_name` read is now threaded end-to-end: proposal "What Changes" ("Map both `updater.rs` OS diagnostics"), design **D6**, the spec requirement renamed to *"OS name and kernel version for diagnostics"*, `tasks.md` 3.1 ("BOTH reassigned reads"), and `implementation.md` "Verified codebase facts › updater.rs" documenting `:410-416` + `:440`. No longer silently inherited.

2. **[was CRITICAL — macOS `OS: unknown` regression]** — **Resolved.** D6 maps the OS name to `sysinfo::System::long_os_version()` (fallback `name()`+`os_version()`). Spec scenario *"Diagnostics include an OS name on both platforms"* asserts a `macOS …` string, NOT `"unknown"`.

3. **[was HIGH — os-string unspecified/untested]** — **Resolved.** Dedicated requirement + two scenarios; `tasks.md` 5.3 (Linux: `OS:` stays distro name) and 5.4 (macOS: `OS:` non-`unknown`) both gate it. The blind spot in the `grep "/proc"` gate is now explicitly covered by the added `grep -rn "os-release\|read_os_pretty_name"` assertion in `tasks.md` 4.1 and `implementation.md` step 5.

4. **[was MEDIUM — macOS `environ` readability]** — **Resolved.** New risk bullet *"macOS cross-process `environ` readability is stricter than cwd/cmd"*, added as spike sub-validation D2a(b), `tasks.md` 1.3(b), and Open Questions (b). The requirement now permits surfacing the limitation rather than a silent `None`, and 5.4 accepts the documented-limitation outcome.

5. **[was MEDIUM — no automated macOS coverage]** — **Resolved.** Stated as an explicit coverage limit in the design risk register, the `tasks.md` blockquote, and `implementation.md` — framed as accepted scope, not implied verification.

6. **[was LOW — single-impl ambiguity]** — **Resolved.** `implementation.md` step 3 now states "The shipped module is single-impl" and reframes the `/proc` mirror as a transient dev-sequencing parity check, with an explicit warning against a lingering `cfg(linux)` `/proc` arm that step 5's grep would wrongly bless.

7. **[was LOW — crate committed before spike]** — **Resolved.** `tasks.md` reordered: 1.2 adds only `sysinfo`, 1.3 runs the spike, 1.4 adds the listener crate "chosen by 1.3(a)". The dep is now added once with the verdict known.

### New issues

8. **[LOW] Spec mandates `PRETTY_NAME` verbatim, but the chosen source won't reproduce it.** The OS-name scenario states "on Linux the `OS:` field SHALL remain the distribution `PRETTY_NAME` (no regression)", yet `sysinfo::System::long_os_version()` returns a *constructed* string (e.g. `"Linux 22.04 Ubuntu"`), not the raw `PRETTY_NAME` (e.g. `"Ubuntu 22.04.4 LTS"`). The design (D6, end) softens this to "confirm the exact string shape in the no-regression walk" — so the design and spec disagree on strictness. Taken literally, a correct implementation could fail the `SHALL`. Recommend relaxing the spec wording to "SHALL remain a meaningful distribution string" (matching the design's intent) so acceptance keys on "non-regression to `unknown`/bare-kernel", not byte-identity with `PRETTY_NAME`. Non-blocking — the behavior is acceptable; only the requirement's phrasing over-constrains.

### Dimension summary
- **Correctness:** Sound. Both reassigned reads now mapped to cross-platform sources; signalling correctly stays `libc::kill(-pgid)` under `cfg(unix)` (D3).
- **Completeness:** The categorical Round 1 gap is closed across proposal→design→spec→tasks→implementation. Only the minor phrasing tension in #8 remains.
- **Concurrency/state:** Unchanged and adequate — single per-tick snapshot reuse, preserved kill ordering, bounded ancestor walk.
- **Test coverage:** Pure logic preserved/gated; macOS execution honestly declared manual-only; OS-string now tested both sides.
- **Security:** No new surface; `environ` privacy/permission consideration now explicitly risk-registered and spike-gated.
- **Rollback:** Per-caller revert intact; the os-release survivor gap that defeated the old grep gate is now caught by the dedicated grep assertion.

All Round 1 blockers are resolved. The single new finding is low-severity wording, not a design defect, and is safely handled at implementation time.

VERDICT: APPROVED
