# Architect brief: quake-always-mounted

Mission: Nergal is a Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19, WebKitGTK). The agent CLI runs in a real PTY rendered by a custom canvas.

- Linear: FRI-475. Tier M, ceremony standard, risk low, files_estimate 5, spec_target `quake-terminal`.
- Trigger: user walk 2026-10-05, the quake slide looks stuck and jerky.
- Root cause (design.md Context): per-open layer build + canvas re-parent + two full repaints in the slide window, and a 200 ms `box-shadow` glow transition (user has `panel_glow: true`) on the sliding element and the panels under it. The exit timer also races the animation.
- Fix: always-mounted overlay with a pinned layer, a four-phase slide machine ended by `animationend`, focus transitions frozen during the slide.
- Gating: OFF (risk low, no migration/security tags). Adversarial review ran once.
- Dependencies: none. Frontend only.
