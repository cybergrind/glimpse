#[derive(Debug)]
pub struct DebounceTracker<T> {
    next_token: u64,
    pending: Option<(u64, T)>,
}

impl<T> Default for DebounceTracker<T> {
    fn default() -> Self {
        Self {
            next_token: 0,
            pending: None,
        }
    }
}

impl<T> DebounceTracker<T> {
    pub fn begin_schedule(&mut self) -> (u64, Option<T>) {
        self.next_token += 1;
        let token = self.next_token;
        let previous = self.pending.take().map(|(_, value)| value);
        (token, previous)
    }

    pub fn commit(&mut self, token: u64, value: T) {
        self.pending = Some((token, value));
    }

    pub fn on_fired(&mut self, token: u64) {
        if self
            .pending
            .as_ref()
            .map(|(pending_token, _)| *pending_token == token)
            .unwrap_or(false)
        {
            self.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DebounceTracker;

    #[test]
    fn clears_pending_entry_when_that_entry_fires() {
        let mut tracker = DebounceTracker::default();

        let (token, previous) = tracker.begin_schedule();
        assert_eq!(previous, None);
        tracker.commit(token, 10);

        tracker.on_fired(token);

        let (token, previous) = tracker.begin_schedule();
        assert_eq!(previous, None);
        tracker.commit(token, 20);
    }

    #[test]
    fn stale_fired_token_does_not_clear_newer_entry() {
        let mut tracker = DebounceTracker::default();

        let (first_token, _) = tracker.begin_schedule();
        tracker.commit(first_token, 10);
        let (second_token, previous) = tracker.begin_schedule();
        assert_eq!(previous, Some(10));
        tracker.commit(second_token, 20);

        tracker.on_fired(first_token);

        let (third_token, previous) = tracker.begin_schedule();
        assert_eq!(previous, Some(20));
        tracker.commit(third_token, 30);
    }
}
