Changes after round 2 (design D6):
- #1 (medium) Failure path keeps data/fetchedAt and clears signal -> retry; scenario + store test.
- #2 (medium) Per-session sequence counter; stale responses dropped; scenario "Out-of-order responses" + test.
- #3 (low) No write for a deleted session; merge refresh only when the session survives the cleanup; test.
- #4 (low) PR_INFO_TTL_MS -> 60 s, single age for shouldRefreshPr and the interval.
- #5 (low) Spec text aligned: check on mount, on signature change, every 60 s.
- Task 1.2 names the legacy StatusLine arm.
