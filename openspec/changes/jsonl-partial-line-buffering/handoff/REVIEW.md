# REVIEW — jsonl-partial-line-buffering

## Reviewer: code-quality-reviewer (single-sequential, sonnet) · 2026-07-03

**Verdict: PASS** — zero findings.

Offset arithmetic walked by hand, both files identical:
- `rposition(b'\n')` None → original offset (whole append held); else
  `complete = buf[..=last_newline]`, `new_offset = offset + complete.len()` — inclusive
  slice correct (len == last_newline+1 lands one past the `\n`); un-terminated tail
  excluded → re-read whole next event, no gap/dup. Off-by-one checked on `b"ab\ncd"`.
- Trailing empty split slice filtered by `trim().is_empty()` — no phantom record.
- `read_to_end` err → original offset (retry); per-line `from_utf8` err → warn+continue
  (batch not aborted).
- **UTF-8 split safety is STRUCTURAL, not luck**: `0x0A` never appears as a non-lead
  byte in a multi-byte sequence and JSON escapes literal control chars, so
  `rposition(b'\n')` only lands on a real record separator; a split multi-byte char is
  by definition in the excluded partial tail.
- send-fail mid-batch returns new_offset (skips remainder) — SAME as pre-change
  behavior (receiver gone = session teardown); not a regression.
- Both `read_appended` byte-for-byte identical in seek/split/utf8/offset logic; only the
  per-line dispatch differs (pi `parse_line`+`wrap`, codex `acc.ingest`).
- 3 tests/file on real temp files (no mocks): half-line held, UTF-8-split recovered,
  N-lines regression (offset == EOF).

## Gates

- Gate 1-3: PASS (clippy -D warnings clean, 766 tests incl. 6 new, fmt clean).
- Gate 6 (scope): 2 files = files_estimate 2.
