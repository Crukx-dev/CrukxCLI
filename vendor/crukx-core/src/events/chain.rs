use crate::events::AgentEvent;
use sha2::{Digest, Sha256};

/// Hash chain fixed point: the "previous hash" of the first event in a
/// session. Any real event hash is 64 hex chars (sha256); this is
/// deliberately not a valid one, so a chain can never accidentally
/// collide with "no prior event."
pub const GENESIS_HASH: &str = "genesis";

fn hash_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One event's hash = sha256(previous_hash || canonical JSON of the
/// event). Chaining on the *previous* hash, not just the event's own
/// content, is what makes this tamper-evident rather than merely
/// checksummed: editing, deleting, or reordering any earlier event
/// changes every hash after it, not just the one that was touched —
/// "history integrity," in the terms of the runtime-verification
/// literature this is inspired by (arxiv 2607.05397).
///
/// Uses `serde_json::to_string` (not pretty-printed) for a canonical
/// encoding — this must serialize identically every time the same event
/// is hashed, which is why this function owns serialization instead of
/// taking a pre-serialized string from a caller who might format it
/// differently.
pub fn hash_event(event: &AgentEvent, previous_hash: &str) -> String {
    let canonical = serde_json::to_string(event).expect("AgentEvent serialization cannot fail");
    hash_hex(format!("{previous_hash}{canonical}").as_bytes())
}

/// The full running chain for a session's events, in order — `chain[i]`
/// is the hash covering `events[0..=i]`, each depending on every hash
/// before it.
pub fn compute_chain(events: &[AgentEvent]) -> Vec<String> {
    let mut chain = Vec::with_capacity(events.len());
    let mut previous = GENESIS_HASH.to_string();
    for event in events {
        let hash = hash_event(event, &previous);
        chain.push(hash.clone());
        previous = hash;
    }
    chain
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainViolation {
    /// Index into `events` where the recorded hash stopped matching the
    /// recomputed one. Everything from here forward is untrustworthy,
    /// even if a later hash happens to look well-formed.
    pub first_divergent_index: usize,
}

/// Recomputes the chain from `events` and compares it to
/// `recorded_hashes` (what was actually stored). `Ok(())` means the
/// history is exactly what it claims to be; `Err` names the first index
/// where they diverge — a stale/mismatched length also counts as
/// divergent at the shorter length, not a separate error case.
pub fn verify_chain(
    events: &[AgentEvent],
    recorded_hashes: &[String],
) -> Result<(), ChainViolation> {
    let recomputed = compute_chain(events);
    let shared_len = recomputed.len().min(recorded_hashes.len());

    for index in 0..shared_len {
        if recomputed[index] != recorded_hashes[index] {
            return Err(ChainViolation {
                first_divergent_index: index,
            });
        }
    }
    if recomputed.len() != recorded_hashes.len() {
        return Err(ChainViolation {
            first_divergent_index: shared_len,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{ActionPhase, AgentAction, AgentAdapter, EVENT_SCHEMA_VERSION};

    fn event(sequence: u64, path: &str) -> AgentEvent {
        AgentEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_id: format!("evt-{sequence}"),
            session_id: "sess-1".to_string(),
            sequence,
            occurred_at: "2026-08-24T00:00:00Z".to_string(),
            adapter: AgentAdapter::Direct,
            repository_root: "/repo".to_string(),
            phase: ActionPhase::Completed,
            action: AgentAction::FileRead {
                path: path.to_string(),
            },
            policy: None,
            error: None,
        }
    }

    #[test]
    fn hash_event_is_deterministic_for_the_same_event_and_previous_hash() {
        let e = event(1, "a.rs");
        assert_eq!(hash_event(&e, GENESIS_HASH), hash_event(&e, GENESIS_HASH));
    }

    #[test]
    fn hash_event_differs_when_the_previous_hash_differs() {
        let e = event(1, "a.rs");
        assert_ne!(
            hash_event(&e, GENESIS_HASH),
            hash_event(&e, "some-other-hash")
        );
    }

    #[test]
    fn compute_chain_produces_one_hash_per_event_each_depending_on_the_last() {
        let events = vec![event(1, "a.rs"), event(2, "b.rs")];
        let chain = compute_chain(&events);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0], hash_event(&events[0], GENESIS_HASH));
        assert_eq!(chain[1], hash_event(&events[1], &chain[0]));
    }

    #[test]
    fn verify_chain_accepts_an_untampered_history() {
        let events = vec![event(1, "a.rs"), event(2, "b.rs"), event(3, "c.rs")];
        let chain = compute_chain(&events);
        assert_eq!(verify_chain(&events, &chain), Ok(()));
    }

    #[test]
    fn verify_chain_detects_a_single_edited_event_and_names_its_index() {
        let mut events = vec![event(1, "a.rs"), event(2, "b.rs"), event(3, "c.rs")];
        let chain = compute_chain(&events);
        // Tamper with the middle event after the chain was computed —
        // exactly what editing a .jsonl file by hand would do.
        events[1] = event(2, "TAMPERED.rs");
        let result = verify_chain(&events, &chain);
        assert_eq!(
            result,
            Err(ChainViolation {
                first_divergent_index: 1
            })
        );
    }

    #[test]
    fn verify_chain_detects_a_deleted_trailing_event() {
        let events = vec![event(1, "a.rs"), event(2, "b.rs"), event(3, "c.rs")];
        let chain = compute_chain(&events);
        let truncated_events = &events[..2];
        let result = verify_chain(truncated_events, &chain);
        assert!(result.is_err());
    }

    #[test]
    fn verify_chain_detects_reordering_even_though_every_event_is_still_present() {
        let events = vec![event(1, "a.rs"), event(2, "b.rs")];
        let chain = compute_chain(&events);
        let reordered = vec![events[1].clone(), events[0].clone()];
        let result = verify_chain(&reordered, &chain);
        assert_eq!(
            result,
            Err(ChainViolation {
                first_divergent_index: 0
            })
        );
    }

    #[test]
    fn empty_events_produce_an_empty_chain_that_verifies_trivially() {
        assert_eq!(compute_chain(&[]), Vec::<String>::new());
        assert_eq!(verify_chain(&[], &[]), Ok(()));
    }
}
