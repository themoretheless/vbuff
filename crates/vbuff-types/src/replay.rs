//! One bounded replay window shared by every anti-replay guard in the
//! workspace.
//!
//! Callback tokens, remote paste leases, webhook events and capability tokens
//! all need the same four properties, and each of them is a security property
//! rather than a matter of style, so they live here once instead of being
//! re-derived per mechanism. It sits in `vbuff-types` because its consumers
//! span `vbuff-ipc` and `vbuff-sync`, which do not depend on each other:
//!
//! * entries expire, so a long-lived process cannot grow an unbounded set;
//! * pruning is never more aggressive than the validity window it protects, so
//!   eviction cannot resurrect a message (argued in [`ReplayGuard::advance_to`]);
//! * the entry count has a hard ceiling and saturation fails closed;
//! * the window's clock never moves backwards, so a rewound caller clock cannot
//!   re-open a window that already closed.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

/// Hard ceiling on live entries in a single replay window.
///
/// Reaching it fails closed: the request under verification is refused instead
/// of being accepted without a replay record, because a one-shot contract that
/// silently stops recording is a repeatable contract.
pub const MAX_REPLAY_ENTRIES: usize = 4_096;

/// Refusal raised when [`ReplayGuard::record`] cannot store an entry.
///
/// Callers map it onto their own fail-closed error vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayWindowFull;

#[derive(Clone)]
struct ReplayEntry<S> {
    state: S,
    expires_at_ms: u64,
}

/// Bounded, monotonically-clocked replay window keyed by `K`.
///
/// `S` carries whatever per-key state its owner needs in order to recognise a
/// replay - the highest sequence number accepted so far, for instance. Plain
/// burn-once nonces use the default `()` state and [`ReplayGuard::burn`].
#[derive(Clone)]
pub struct ReplayGuard<K, S = ()> {
    entries: BTreeMap<K, ReplayEntry<S>>,
    /// Highest `now_ms` ever observed by [`Self::advance_to`]. Time is clamped
    /// to this floor so a rewound caller clock cannot make an already evicted
    /// entry look fresh again.
    clock_floor_ms: u64,
    capacity: usize,
}

impl<K, S> fmt::Debug for ReplayGuard<K, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Keys are nonces, request hashes and endpoint hashes; only the shape
        // of the window is printable.
        formatter
            .debug_struct("ReplayGuard")
            .field("entries", &self.entries.len())
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl<K: Ord, S> ReplayGuard<K, S> {
    /// Creates an empty window that refuses to hold more than `capacity` live
    /// entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            clock_floor_ms: 0,
            capacity,
        }
    }

    /// Moves the window's clock to `now_ms`, drops every entry that can no
    /// longer matter, and returns the clock the caller must use for the rest of
    /// the verification.
    ///
    /// Retention is exactly the validity window of what the entry protects,
    /// never shorter: an entry is stored under the `expires_at_ms` its owner
    /// passed to [`Self::record`] and dropped only once `now_ms >=
    /// expires_at_ms`, which is precisely the point where the same message
    /// starts failing its own freshness check. So eviction can never resurrect
    /// a message:
    ///
    /// * while `now_ms < expires_at_ms` the entry is still present and the
    ///   replay is refused on the entry;
    /// * once the entry is evicted, `now_ms >= expires_at_ms` held at that
    ///   moment, and the returned clock is clamped to a non-decreasing floor,
    ///   so every later call refuses the message as expired.
    ///
    /// The clamp matters because eviction is the only thing that makes replay
    /// rejection depend on the clock; without it a caller whose wall clock
    /// jumped backwards could re-present an already forgotten key inside a
    /// re-opened validity window.
    ///
    /// Memory is therefore bounded by the acceptance rate over the longest
    /// validity window, and hard-capped by `capacity` fail-closed.
    pub fn advance_to(&mut self, now_ms: u64) -> u64 {
        self.clock_floor_ms = self.clock_floor_ms.max(now_ms);
        let now_ms = self.clock_floor_ms;
        self.entries.retain(|_, entry| entry.expires_at_ms > now_ms);
        now_ms
    }

    /// State recorded for `key`, if it is still inside its validity window.
    ///
    /// Only meaningful right after [`Self::advance_to`]; stale entries are
    /// pruned there, not here.
    pub fn state(&self, key: &K) -> Option<&S> {
        self.entries.get(key).map(|entry| &entry.state)
    }

    /// Whether `key` has already been recorded and is still live.
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// Number of live entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the window currently holds no live entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records `state` for `key` until `expires_at_ms`.
    ///
    /// `expires_at_ms` must be the instant at which the caller's own freshness
    /// check starts refusing the message that is being recorded; that equality
    /// is what makes eviction safe (see [`Self::advance_to`]).
    ///
    /// Re-recording a live key extends its retention and never shortens it, so
    /// a key shared by several messages (a webhook endpoint, say) stays covered
    /// until the longest-lived message it accepted has expired. Fails closed
    /// with [`ReplayWindowFull`] when a *new* key would exceed the capacity.
    pub fn record(&mut self, key: K, state: S, expires_at_ms: u64) -> Result<(), ReplayWindowFull> {
        let saturated = self.entries.len() >= self.capacity;
        match self.entries.entry(key) {
            Entry::Occupied(mut slot) => {
                let slot = slot.get_mut();
                slot.state = state;
                slot.expires_at_ms = slot.expires_at_ms.max(expires_at_ms);
                Ok(())
            }
            Entry::Vacant(slot) => {
                if saturated {
                    return Err(ReplayWindowFull);
                }
                slot.insert(ReplayEntry {
                    state,
                    expires_at_ms,
                });
                Ok(())
            }
        }
    }
}

impl<K: Ord> ReplayGuard<K> {
    /// Burns a one-shot `key` until `expires_at_ms`.
    pub fn burn(&mut self, key: K, expires_at_ms: u64) -> Result<(), ReplayWindowFull> {
        self.record(key, (), expires_at_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_their_window_and_are_dropped_exactly_at_expiry() {
        let mut guard = ReplayGuard::<u8>::new(4);
        guard.advance_to(10);
        guard.burn(1, 100).unwrap();

        assert_eq!(guard.advance_to(99), 99);
        assert!(guard.contains(&1));
        assert_eq!(guard.advance_to(100), 100);
        assert!(!guard.contains(&1));
        assert!(guard.is_empty());
    }

    #[test]
    fn clock_never_moves_backwards() {
        let mut guard = ReplayGuard::<u8>::new(4);
        guard.advance_to(500);
        guard.burn(1, 600).unwrap();

        // A rewound caller clock is clamped, so the entry is still pruned on
        // schedule and the caller sees the floor rather than its own clock.
        assert_eq!(guard.advance_to(10), 500);
        assert!(guard.contains(&1));
        assert_eq!(guard.advance_to(600), 600);
        assert_eq!(guard.advance_to(0), 600);
        assert!(!guard.contains(&1));
    }

    #[test]
    fn re_recording_a_key_extends_retention_and_never_shortens_it() {
        let mut guard = ReplayGuard::<u8, u64>::new(4);
        guard.record(1, 7, 1_000).unwrap();
        guard.record(1, 8, 200).unwrap();

        assert_eq!(guard.state(&1).copied(), Some(8));
        guard.advance_to(999);
        assert_eq!(guard.state(&1).copied(), Some(8));
        guard.advance_to(1_000);
        assert!(guard.state(&1).is_none());
    }

    #[test]
    fn saturation_fails_closed_for_new_keys_only() {
        let mut guard = ReplayGuard::<u8, u64>::new(2);
        guard.record(1, 1, 100).unwrap();
        guard.record(2, 2, 100).unwrap();

        assert_eq!(guard.record(3, 3, 100), Err(ReplayWindowFull));
        // A live key can still be updated while saturated, otherwise a busy
        // endpoint would lose its own monotonic counter.
        guard.record(2, 9, 100).unwrap();
        assert_eq!(guard.state(&2).copied(), Some(9));

        guard.advance_to(100);
        guard.record(3, 3, 200).unwrap();
        assert_eq!(guard.len(), 1);
    }

    #[test]
    fn debug_reveals_shape_but_never_keys() {
        let mut guard = ReplayGuard::<u64>::new(8);
        guard.burn(0xdead_beef, 100).unwrap();
        let rendered = format!("{guard:?}");
        assert_eq!(rendered, "ReplayGuard { entries: 1, capacity: 8 }");
    }
}
