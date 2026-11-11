use crate::config::capture::{RETRY_INITIAL_SECS, RETRY_MAX_SECS};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::time::{Duration, Instant};

/// Keep capture callbacks non-blocking while preferring fresh frames over backlog.
pub fn send_latest<T>(tx: &Sender<T>, discard: &Receiver<T>, value: T) -> u64 {
    match tx.try_send(value) {
        Ok(()) => 0,
        Err(TrySendError::Disconnected(_)) => 1,
        Err(TrySendError::Full(value)) => {
            let dropped = u64::from(discard.try_recv().is_ok());
            dropped + u64::from(tx.try_send(value).is_err())
        }
    }
}

#[derive(Default)]
pub struct RetryPolicy {
    next: Option<Instant>,
    attempts: u32,
}

impl RetryPolicy {
    pub fn arm(&mut self, now: Instant) {
        self.next = Some(now + Duration::from_secs(RETRY_INITIAL_SECS));
        self.attempts = 0;
    }
    pub fn cancel(&mut self) {
        self.next = None;
        self.attempts = 0;
    }
    pub fn active(&self) -> bool {
        self.next.is_some()
    }
    pub fn due(&self, now: Instant) -> bool {
        self.next.is_some_and(|next| now >= next)
    }
    pub fn failed(&mut self, now: Instant) {
        self.attempts = (self.attempts + 1).min(4);
        self.next = Some(
            now + Duration::from_secs((RETRY_INITIAL_SECS << self.attempts).min(RETRY_MAX_SECS)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overloaded_consumer_receives_freshest_frames() {
        let (tx, rx) = crossbeam_channel::bounded(2);
        assert_eq!(send_latest(&tx, &rx, 1), 0);
        send_latest(&tx, &rx, 2);
        assert_eq!(send_latest(&tx, &rx, 3), 1);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [2, 3]);
    }
    #[test]
    fn manual_stop_cancels_reconnect_and_backoff_is_bounded() {
        let now = Instant::now();
        let mut retry = RetryPolicy::default();
        retry.arm(now);
        assert!(!retry.due(now));
        assert!(retry.due(now + Duration::from_secs(2)));
        for _ in 0..100 {
            retry.failed(now);
        }
        assert!(!retry.due(now + Duration::from_secs(29)));
        assert!(retry.due(now + Duration::from_secs(30)));
        retry.cancel();
        assert!(!retry.active());
        assert!(!retry.due(now + Duration::from_secs(1000)));
    }
}
