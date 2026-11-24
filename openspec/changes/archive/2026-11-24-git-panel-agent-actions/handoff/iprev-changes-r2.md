Changes since round 1 (one line per finding):
1. Leader guard: the handler returns first on `e.defaultPrevented` or `leaderPendingAtom`; new requirement "The M merge key is guarded" + scenario "Leader m does not open the merge confirmation"; D1 no longer claims a listener order; walk 6.3.
2. Stale closures: latest-value refs for state/merging/performMerge, re-check after confirm; scenario "PR merged while the confirmation is open".
3. ship-flow delta added (Apply-with-Agent label); conflict-resolution "Contextual resolve-conflict shortcut dispatch" now names "Ask Agent".
4. Conflicts file picker added to the hint bar requirement and task 3.6; inline hints (:1057, :1456) stay inline at 10px (requirement "The PR viewer lists its keys" carries that rule).
5. useConflictResolution.ts added to Impact and task 1.4; comments at PrViewer :264/:716 in task 1.3.
6. Helper ctx gains canMerge; Red test 2.1 covers canMerge, repeat, Space; walks 6.3/6.5 cover leader m, merged PR, terminal focus, annotation-count body.
7. Shift+M accepted (consistent with `a`, chip shows upper-case M); D1 records the alternative.
8. mergeConfirmOpenRef + e.repeat guard; scenario "A held M opens one confirmation".
9. deliver_prompt_to_session is now async and resolves after the CR, returning submit errors (core `deliver_prompt(pty, sid, text, settle)` testable on PtyManager::new()); Apply gets an `applying` guard; Ask's `sending` now spans the delivery; half-typed CC input submitted along is an accepted risk in design.
10. Line refs corrected.
11. Task 4.4 removed.
Also: long requirements split (validate warnings).
