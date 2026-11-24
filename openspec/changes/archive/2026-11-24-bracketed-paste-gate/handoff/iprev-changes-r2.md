Changes after round 1:
- #1 (high) PrViewer "Apply with Claude" moves to a new terminal_deliver command backed by paste_to_session; spec scenario names it; task 1.3; walk 2.4 step.
- #2 (medium) user_paste_payload(session, text) links the mode read to the encoding; tests on a session with and without ESC[?2004h.
- #3 (medium) MODIFIED "A paste body cannot end the bracketed paste" to cover both encoders, with a plain-mode scenario (U+009B + rebuilt marker); test case added.
- #4 (low) Order now newlines then markers (matches wezterm); \r\r\n test added.
- #5 (low) Aux tracker gap documented in Risks; terminal_paste resets start_col after a plain paste with a newline.
- #6 (low) One session-lock scope for the mode and cursor_col.
- #7, #8: no change needed.
