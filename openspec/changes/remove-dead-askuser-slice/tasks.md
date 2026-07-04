## 0. Gate

- [x] 0.1 Confirm with the user: delete the parked `AskUserModalLegacy` revival option (memory says "kept in case we revive"). Do not proceed without an explicit yes.

## 1. Frontend deletion (leaf-first)

- [x] 1.1 `src/App.tsx`: remove the `AskUserModal` import (4) and mount (162).
- [x] 1.2 Delete `src/components/session/AskUserModal.tsx` (null export + `AskUserModalLegacy`).
- [x] 1.3 `src/stores/askUser.ts`: delete `askUserAtom` + its state types + the stub comment (16-19); keep `pendingAsksAtom`/`pendingAttentionAtom` and their types.

## 2. Backend deletion (compiler-guided)

- [x] 2.1 Grep-verify zero remaining callers: `"submit_ask_answer"` (string + symbol) in `src/` and `src-tauri/`; `pending_ask_fifos` registration entry points beyond the adapter itself.
- [x] 2.2 `src-tauri/src/lib.rs`: remove the `commands::submit_ask_answer` invoke_handler entry (416).
- [x] 2.3 `src-tauri/src/commands.rs`: delete `submit_ask_answer` (378+).
- [x] 2.4 `src-tauri/src/agents/claude_code/adapter.rs`: delete the `submit_ask_answer` override (300+), the `pending_ask_fifos` field (43), its init (68) and registration (81).
- [x] 2.5 `src-tauri/src/agents/mod.rs`: delete the trait method + default (643-652).
- [x] 2.6 Test: assert CC's `capabilities()` still declares `ASK_USER_BLOCKING` (flag survives the method removal).

## 3. Doc correction

- [x] 3.1 Rewrite `agents/claude_code/adapter.rs:5-20` module doc: plan-review FIFOs live; ask-user is a non-blocking notifier; the dialog flow was removed (this change), git history holds the implementation.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 4.2 `npx tsc --noEmit`
- [ ] 4.3 Manual: trigger an AskUserQuestion in a CC session → tab blinks/tints, answering in the TUI clears it (attention flow intact).
