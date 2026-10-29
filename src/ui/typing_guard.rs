//! The typing guard (`composerTypingGuard.ts`): a request that arrives
//! while the user types in the composer waits until 1.5 s after the last
//! keystroke (or a blur or a send) before it takes the composer over, so a
//! sentence is not cut in half. A request seen once is never held later.

use std::collections::HashSet;
use std::time::{Duration, Instant};

/// `COMPOSER_TYPING_IDLE_MS`.
pub(crate) const IDLE: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub(crate) struct TypingGuard {
    /// The composer it watches (a thread id); another one starts afresh.
    scope: String,
    seen: HashSet<String>,
    held: HashSet<String>,
    focused: bool,
    last_typed: Option<Instant>,
}

impl TypingGuard {
    /// The requests now pending: new ones are held while the user types;
    /// returns those held.
    pub(crate) fn requests(
        &mut self,
        scope: &str,
        ids: &[String],
        now: Instant,
    ) -> &HashSet<String> {
        if self.scope != scope {
            let focused = self.focused;
            *self = Self {
                scope: scope.to_owned(),
                focused,
                ..Self::default()
            };
        }
        if self
            .last_typed
            .is_some_and(|at| now.duration_since(at) >= IDLE)
        {
            self.idle();
        }
        self.held.retain(|id| ids.contains(id));
        let typing = self.focused
            && self
                .last_typed
                .is_some_and(|at| now.duration_since(at) < IDLE);
        for id in ids {
            if self.seen.insert(id.clone()) && typing {
                self.held.insert(id.clone());
            }
        }
        &self.held
    }

    pub(crate) fn typed(&mut self, now: Instant) {
        self.last_typed = Some(now);
    }

    pub(crate) fn focus(&mut self, focused: bool) {
        if self.focused && !focused {
            self.idle();
        }
        self.focused = focused;
    }

    /// A send or the idle time: what was held shows.
    pub(crate) fn idle(&mut self) {
        self.held.clear();
        self.last_typed = None;
    }

    /// When the held requests show, for a repaint.
    pub(crate) fn release_in(&self, now: Instant) -> Option<Duration> {
        (!self.held.is_empty())
            .then(|| {
                self.last_typed
                    .map(|at| IDLE.saturating_sub(now.duration_since(at)))
            })
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_arriving_while_typing_waits_for_the_pause() {
        let mut guard = TypingGuard::default();
        let start = Instant::now();
        guard.focus(true);
        guard.requests("t", &[], start);
        guard.typed(start);
        let ids = vec!["q".to_owned()];
        assert!(
            guard
                .requests("t", &ids, start + Duration::from_millis(200))
                .contains("q")
        );
        // Still typing: still held.
        guard.typed(start + Duration::from_millis(1000));
        assert!(
            guard
                .requests("t", &ids, start + Duration::from_millis(2000))
                .contains("q")
        );
        // The pause: it shows, and is never held again.
        assert!(
            guard
                .requests("t", &ids, start + Duration::from_millis(2600))
                .is_empty()
        );
        guard.typed(start + Duration::from_millis(2700));
        assert!(
            guard
                .requests("t", &ids, start + Duration::from_millis(2800))
                .is_empty()
        );
        // Not typing: a new request shows at once; a blur releases.
        let more = vec!["q".to_owned(), "a".to_owned()];
        assert!(
            guard
                .requests("t", &more, start + Duration::from_secs(9))
                .is_empty()
        );
        guard.typed(start + Duration::from_secs(10));
        let next = vec!["b".to_owned()];
        assert!(
            !guard
                .requests("t", &next, start + Duration::from_secs(10))
                .is_empty()
        );
        guard.focus(false);
        assert!(
            guard
                .requests("t", &next, start + Duration::from_secs(10))
                .is_empty()
        );
    }
}
