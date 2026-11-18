# Architect brief — terminal-cc-compat

Project mission: Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers.

Linear: FRI-502. Control metadata: tier M, ceremony standard, risk low, files_estimate 5, spec_target `terminal-wezterm`.

## Context

FRI-502 asks to verify 7 terminal points against Claude Code v2.1.247–2.1.287. The audit result is `handoff/terminal-matrix.md`.

## Scope decision

- Fixed here: focus reports, middle/right button forwarding, paste marker stripping, whole-message PTY writes. All are additive and opt-in by the running app (DECSET 1004, mouse modes) or pure hardening.
- Split out: kitty key encoding (FRI-503) changes the bytes of every modified key sent to CC; OSC 10/11 theme colors (FRI-504) need a frontend→backend theme sync; DEC 2026 in the emitter (FRI-505) changes the emit loop. Each one is a separate contract or a scope jump over 20%.
- Open question: should `terminal_paste` wrap only when the app enabled DECSET 2004 (xterm behavior)? It changes the spec'd "Bracketed paste" scenario, so it waits for the user.

## Gating

files_estimate 5 → triple-prompt gating ON by rule; run as a single adversarial review because the diff is small and low risk. No iprev (no migration/security/breaking tags).

## Lazy skills

None applied: no UI surface beyond event listeners on the existing canvas.
