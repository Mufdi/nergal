Changes since round 2:
1. applyingRef checked/set synchronously on entry (state only drives disabled); walk 6.6 adds Ctrl+Shift+R twice.
2. requestMergeConfirm captures {workspaceId, prNumber} and aborts after confirm if they changed; new scenario "Displayed PR changes while the confirmation is open".
3. Task 1.1 adds the happy-path tokio test with a capturing SharedWriter on a real PtyInstance (paste bytes then trailing \r, written before the future resolves).
4. ctx.canMerge built from the refs.
5. Helper type includes repeat.
6. interview_answers.apply_submit records the mechanism refinement; spec_target includes ship-flow.
7. Ask Agent moves focus to the terminal after the delivery resolves.
