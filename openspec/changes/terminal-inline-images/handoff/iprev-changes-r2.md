Changes since round 1 (one line per finding):
1. terminal_image is async: clone the session Arc, drop session_ptys/instances, upgrade Weak under the session mutex, build payload in spawn_blocking (design D2, impl step 4, spec "SHALL NOT block the main thread").
2. Decode failures permanent per hash; transport failures retried at most twice; resetFailures removed (D4, impl 7, spec, task 3.2).
3. Magic-byte allowlist PNG/JPEG/GIF for kind 1; other encoded formats error (D3 security note, spec scenario "Unsupported encoded format", task 2.1).
4. Eviction never drops a hash touched in the current paint pass (beginPass/touch); overshoot allowed until the pass ends (D4, spec scenario, task 3.2).
5. dispose sets a flag; late results close their bitmap without ready; both destruction sites named (destroy :516, quake shell removal :532-537).
6. applyRowImages pure helper (row.images ?? [], reshape, trim to cols) with tests (impl 7, task 3.1).
7. Test 2.1: registry unit test with an owned Arc dropped + ESC[2J then 16 distinct sixels evicting the first.
8. Partial overwrite test added to 1.1 (two runs, contiguous u).
9. Padding semantics follow wezterm per-cell values (left/top every cell); merge only on equal left/top; risk for FRI-613 recorded; ImageData line refs fixed.
10. GridSnapshot.images readers use .get(i) with empty fallback.
11. col documented as terminal column; wide-char risk recorded.
12. Hash hex {:02x} 64 chars (D2, test 1.1).
13. Padding-at-placement-time risk + walk 4.5 zooms the font.
14. Gate base unified to the stacked FRI-609 branch.
