//! Browser-style back/forward history of viewed tabs.
//!
//! This is TUI presentation state: each client records the tabs it has shown, keyed by
//! endpoint so entries on different machines with the same tab ID stay distinct.

use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 100;
/// A tab viewed for less than this is treated as passed through, not visited. This folds
/// rapid cycling and the transient tab shown while a remote machine activation settles.
const DWELL: Duration = Duration::from_millis(750);
/// How long a history jump waits for its target to become the viewed tab.
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub(super) struct TabHistory<K> {
    entries: Vec<K>,
    /// Index of the entry currently being viewed.
    index: usize,
    last_recorded_at: Option<Instant>,
    /// A jump in flight: the target index and when it was requested.
    navigating: Option<(usize, Instant)>,
}

impl<K> Default for TabHistory<K> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: 0,
            last_recorded_at: None,
            navigating: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HistoryDirection {
    Back,
    Forward,
}

impl<K: Clone + PartialEq> TabHistory<K> {
    /// Records that `key` is now the viewed tab.
    pub(super) fn record(&mut self, key: K, now: Instant) {
        if let Some((target, requested_at)) = self.navigating {
            if self.entries.get(target) == Some(&key) {
                self.navigating = None;
                self.index = target;
                self.last_recorded_at = Some(now);
                return;
            }
            if now.duration_since(requested_at) < NAVIGATION_TIMEOUT {
                // Intermediate views while the jump settles are not visits.
                return;
            }
            self.navigating = None;
        }
        if self.entries.get(self.index) == Some(&key) {
            return;
        }
        let at_end = self.index + 1 == self.entries.len();
        let passed_through = at_end
            && self
                .last_recorded_at
                .is_some_and(|recorded| now.duration_since(recorded) < DWELL);
        self.entries.truncate(self.index + 1);
        if passed_through {
            self.entries.pop();
        }
        // Leaving a duplicate earlier would make back walk through the same tab twice.
        if let Some(position) = self.entries.iter().position(|entry| entry == &key) {
            self.entries.remove(position);
        }
        self.entries.push(key);
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.index = self.entries.len() - 1;
        self.last_recorded_at = Some(now);
    }

    /// Starts a jump and returns its target, skipping entries `available` rejects.
    /// Repeated calls before the jump lands continue from the pending target.
    pub(super) fn navigate(
        &mut self,
        direction: HistoryDirection,
        now: Instant,
        mut available: impl FnMut(&K) -> bool,
    ) -> Option<K> {
        let from = match self.navigating {
            Some((target, requested_at))
                if now.duration_since(requested_at) < NAVIGATION_TIMEOUT =>
            {
                target
            }
            _ => self.index,
        };
        let mut candidate = from;
        loop {
            candidate = match direction {
                HistoryDirection::Back => candidate.checked_sub(1)?,
                HistoryDirection::Forward => {
                    let next = candidate + 1;
                    (next < self.entries.len()).then_some(next)?
                }
            };
            let key = &self.entries[candidate];
            if available(key) {
                self.navigating = Some((candidate, now));
                return Some(key.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(
        history: &mut TabHistory<&'static str>,
        keys: &[&'static str],
        start: Instant,
    ) -> Instant {
        let mut now = start;
        for key in keys {
            now += Duration::from_secs(1);
            history.record(key, now);
        }
        now
    }

    fn back(history: &mut TabHistory<&'static str>, now: Instant) -> Option<&'static str> {
        history.navigate(HistoryDirection::Back, now, |_| true)
    }

    fn forward(history: &mut TabHistory<&'static str>, now: Instant) -> Option<&'static str> {
        history.navigate(HistoryDirection::Forward, now, |_| true)
    }

    #[test]
    fn back_and_forward_walk_visited_tabs() {
        let mut history = TabHistory::default();
        let mut now = visit(&mut history, &["a", "b", "c"], Instant::now());
        now += Duration::from_secs(1);
        assert_eq!(back(&mut history, now), Some("b"));
        history.record("b", now);
        assert_eq!(back(&mut history, now), Some("a"));
        history.record("a", now);
        assert_eq!(back(&mut history, now), None);
        assert_eq!(forward(&mut history, now), Some("b"));
        history.record("b", now);
        assert_eq!(forward(&mut history, now), Some("c"));
        history.record("c", now);
        assert_eq!(forward(&mut history, now), None);
    }

    #[test]
    fn repeated_presses_before_focus_lands_keep_walking() {
        let mut history = TabHistory::default();
        let now = visit(&mut history, &["a", "b", "c", "d"], Instant::now());
        assert_eq!(back(&mut history, now), Some("c"));
        assert_eq!(back(&mut history, now), Some("b"));
        assert_eq!(back(&mut history, now), Some("a"));
        history.record("a", now);
        assert_eq!(forward(&mut history, now), Some("b"));
    }

    #[test]
    fn visiting_after_going_back_drops_forward_entries() {
        let mut history = TabHistory::default();
        let mut now = visit(&mut history, &["a", "b", "c"], Instant::now());
        assert_eq!(back(&mut history, now), Some("b"));
        history.record("b", now);
        now = visit(&mut history, &["x"], now);
        assert_eq!(forward(&mut history, now), None);
        assert_eq!(back(&mut history, now), Some("b"));
    }

    #[test]
    fn intermediate_views_during_a_jump_are_ignored() {
        let mut history = TabHistory::default();
        let now = visit(
            &mut history,
            &["local", "remote-a", "local-2"],
            Instant::now(),
        );
        assert_eq!(back(&mut history, now), Some("remote-a"));
        // Activating the remote machine first shows its previously focused tab.
        history.record("remote-b", now + Duration::from_millis(10));
        history.record("remote-a", now + Duration::from_millis(50));
        assert_eq!(
            back(&mut history, now + Duration::from_secs(1)),
            Some("local")
        );
    }

    #[test]
    fn a_failed_jump_expires() {
        let mut history = TabHistory::default();
        let now = visit(&mut history, &["a", "b"], Instant::now());
        assert_eq!(back(&mut history, now), Some("a"));
        let later = now + NAVIGATION_TIMEOUT + Duration::from_secs(1);
        history.record("c", later);
        assert_eq!(back(&mut history, later), Some("b"));
    }

    #[test]
    fn briefly_viewed_tabs_are_not_kept() {
        let mut history = TabHistory::default();
        let mut now = visit(&mut history, &["a", "b"], Instant::now()) + DWELL;
        // Cycling through c and d quickly to land on e.
        for key in ["c", "d", "e"] {
            now += Duration::from_millis(100);
            history.record(key, now);
        }
        assert_eq!(back(&mut history, now + DWELL), Some("b"));
    }

    #[test]
    fn revisiting_a_tab_moves_it_to_the_end() {
        let mut history = TabHistory::default();
        let now = visit(&mut history, &["a", "b", "a"], Instant::now());
        assert_eq!(back(&mut history, now), Some("b"));
        history.record("b", now);
        assert_eq!(back(&mut history, now), None);
    }

    #[test]
    fn unavailable_entries_are_skipped() {
        let mut history = TabHistory::default();
        let now = visit(&mut history, &["a", "closed", "c"], Instant::now());
        assert_eq!(
            history.navigate(HistoryDirection::Back, now, |key| *key != "closed"),
            Some("a")
        );
    }

    #[test]
    fn history_is_bounded() {
        let mut history = TabHistory::default();
        let mut now = Instant::now();
        for index in 0..MAX_ENTRIES + 10 {
            now += Duration::from_secs(1);
            history.record(index, now);
        }
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert_eq!(history.entries[0], 10);
    }
}
