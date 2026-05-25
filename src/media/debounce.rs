use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::media::event::MediaEvent;

pub struct Debouncer {
    delay: Duration,
    pending: Option<JoinHandle<()>>,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: None,
        }
    }

    /// Submit an event. Aborts any pending timer and starts a new one.
    /// After the delay, the event is sent on `tx`.
    pub fn submit(&mut self, event: MediaEvent, tx: mpsc::Sender<MediaEvent>) {
        if let Some(handle) = self.pending.take() {
            handle.abort();
        }
        let delay = self.delay;
        self.pending = Some(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx.send(event).await;
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::event::MediaEvent;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn debouncer_emits_after_delay() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut debouncer = Debouncer::new(Duration::from_millis(50));
        debouncer.submit(MediaEvent::PlaybackPaused, tx);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn debouncer_aborts_stale_event() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut debouncer = Debouncer::new(Duration::from_millis(100));

        // Submit track A
        debouncer.submit(
            MediaEvent::TrackChanged {
                title: "A".into(),
                artist: "X".into(),
                album: "Y".into(),
                elapsed_secs: None,
                duration_secs: None,
            },
            tx.clone(),
        );

        // Advance only 50ms — not enough to fire
        tokio::time::advance(Duration::from_millis(50)).await;

        // Submit track B (should abort A)
        debouncer.submit(
            MediaEvent::TrackChanged {
                title: "B".into(),
                artist: "X".into(),
                album: "Y".into(),
                elapsed_secs: None,
                duration_secs: None,
            },
            tx.clone(),
        );

        // Advance enough for B to fire
        tokio::time::advance(Duration::from_millis(150)).await;

        // Should receive exactly one event
        let event = rx.recv().await.expect("should have received one event");
        match event {
            MediaEvent::TrackChanged { title, .. } => assert_eq!(title, "B"),
            _ => panic!("expected TrackChanged B"),
        }
        // Should NOT receive a second event (A was aborted)
        assert!(rx.try_recv().is_err(), "should not receive stale event A");
    }
}
