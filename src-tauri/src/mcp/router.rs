//! Pure thread-router logic for cross-session-messaging: dedup keying, reach
//! hop-cap depth, and budget evaluation. No I/O — the orchestration in
//! `messaging.rs` feeds it loaded thread/message state so every cap is
//! unit-testable in isolation (tasks §7.1).

use sha2::{Digest, Sha256};

/// Conservative body normalization for dedup: trim + collapse internal runs of
/// whitespace to a single space. Documented limitation: a reworded follow-up is
/// NOT deduped — the reach hop cap + msg budget are the backstops, not this.
pub fn normalize_body(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Exact-match dedup key over (from, to, normalized body). SHA-256 (not the
/// std `DefaultHasher`, whose SipHash seed is randomized per process) so the key
/// stored in the DB stays stable across app restarts — otherwise an identical
/// message would re-deliver after every restart. A `\0` separator between fields
/// keeps `a|bc` from colliding with `ab|c`.
pub fn dedup_key(from: &str, to: &str, body: &str) -> String {
    let mut h = Sha256::new();
    h.update(from.as_bytes());
    h.update([0u8]);
    h.update(to.as_bytes());
    h.update([0u8]);
    h.update(normalize_body(body).as_bytes());
    format!("{:x}", h.finalize())
}

/// The reach level at which `sender` joined the thread: 0 for the originator,
/// otherwise the depth of the earliest message that reached them. `messages` is
/// `(to_session, depth)` for the thread. An unknown sender (no inbound message,
/// not originator) is treated as level 0 — it cannot send before being reached,
/// so this only matters defensively.
pub fn sender_level(originator: &str, sender: &str, messages: &[(String, u32)]) -> u32 {
    if sender == originator {
        return 0;
    }
    messages
        .iter()
        .filter(|(to, _)| to == sender)
        .map(|(_, d)| *d)
        .min()
        .unwrap_or(0)
}

/// Per-message reach depth: the sender's own level plus one only when the target
/// is NOT yet a thread participant. A reply between existing participants does
/// not increment reach (that conversation is bounded by `msg_budget`).
pub fn reach_depth(sender_level: u32, target_is_new_participant: bool) -> u32 {
    sender_level + u32::from(target_is_new_participant)
}

/// Whether pulling in a new participant at `depth` would breach the reach cap.
/// Only a NEW participant can breach it — a reply never increments reach.
pub fn exceeds_hop_cap(depth: u32, target_is_new_participant: bool, max_hops: u32) -> bool {
    target_is_new_participant && depth > max_hops
}

/// Whether appending one more message exhausts the count budget (the new count
/// would meet or exceed `msg_budget`). Evaluated AFTER the message is counted.
pub fn budget_exhausted(new_msg_count: u32, msg_budget: u32) -> bool {
    msg_budget > 0 && new_msg_count >= msg_budget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_and_trims() {
        assert_eq!(normalize_body("  hi   there\n\tyou  "), "hi there you");
    }

    #[test]
    fn dedup_key_is_stable_under_whitespace_only_changes() {
        let a = dedup_key("A", "B", "ship  the\tfix");
        let b = dedup_key("A", "B", " ship the fix ");
        assert_eq!(a, b, "whitespace-only differences dedupe");
    }

    #[test]
    fn dedup_key_differs_on_reword_and_on_direction() {
        assert_ne!(
            dedup_key("A", "B", "ship it"),
            dedup_key("A", "B", "ship now")
        );
        assert_ne!(
            dedup_key("A", "B", "ship it"),
            dedup_key("B", "A", "ship it"),
            "direction is part of the key"
        );
        // Field separator guards against boundary collisions.
        assert_ne!(dedup_key("a", "bc", "x"), dedup_key("ab", "c", "x"));
    }

    #[test]
    fn originator_is_level_zero() {
        assert_eq!(sender_level("A", "A", &[]), 0);
    }

    #[test]
    fn sender_level_is_earliest_inbound_depth() {
        // A→B(1), B→C(2), C→B(2): B's level is its earliest inbound = 1.
        let msgs = vec![
            ("B".to_string(), 1),
            ("C".to_string(), 2),
            ("B".to_string(), 2),
        ];
        assert_eq!(sender_level("A", "B", &msgs), 1);
        assert_eq!(sender_level("A", "C", &msgs), 2);
    }

    #[test]
    fn reach_increments_only_for_new_participant() {
        assert_eq!(reach_depth(1, true), 2, "new participant +1");
        assert_eq!(reach_depth(1, false), 1, "reply does not increment");
    }

    #[test]
    fn hop_cap_only_bites_new_participants() {
        // A↔B ping-pong at level 0/1 never breaches even a cap of 1.
        assert!(!exceeds_hop_cap(0, false, 1));
        assert!(!exceeds_hop_cap(1, false, 1));
        // A→B→C→D→E with max_hops 4: E joins at depth 4 (ok), a 6th hop at 5 breaches.
        assert!(!exceeds_hop_cap(4, true, 4));
        assert!(exceeds_hop_cap(5, true, 4));
    }

    #[test]
    fn budget_exhausts_at_cap() {
        assert!(!budget_exhausted(29, 30));
        assert!(budget_exhausted(30, 30));
        assert!(budget_exhausted(31, 30));
        assert!(!budget_exhausted(99, 0), "0 budget = unbounded");
    }
}
