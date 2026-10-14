use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{Builder, JoinHandle};

/// A background thread that runs until it is asked to stop.
pub(super) struct Worker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Worker {
    /// Runs `body` on a named thread; `body` must return soon after the flag becomes true.
    pub fn spawn(
        name: &str,
        body: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
    ) -> io::Result<Worker> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = Builder::new()
            .name(name.to_string())
            .spawn(move || body(thread_stop))?;
        Ok(Worker { stop, handle })
    }

    /// Signals the thread and waits for it to finish.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        // A panicked worker has nothing left to clean up, so the join result is not needed.
        let _ = self.handle.join();
    }
}

pub(super) fn is_stopped(stop: &AtomicBool) -> bool {
    stop.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn stop_signals_the_body_and_waits_for_it_to_finish() {
        let finished = Arc::new(AtomicBool::new(false));
        let thread_finished = Arc::clone(&finished);
        let worker = Worker::spawn("test-worker", move |stop| {
            while !is_stopped(&stop) {
                thread::sleep(Duration::from_millis(1));
            }
            thread_finished.store(true, Ordering::SeqCst);
        })
        .expect("spawn");

        assert!(!finished.load(Ordering::SeqCst));
        worker.stop();

        assert!(finished.load(Ordering::SeqCst));
    }

    #[test]
    fn the_thread_gets_the_requested_name() {
        let names = Arc::new(std::sync::Mutex::new(Vec::new()));
        let thread_names = Arc::clone(&names);
        let worker = Worker::spawn("named-worker", move |_| {
            thread_names
                .lock()
                .expect("lock")
                .push(thread::current().name().map(str::to_string));
        })
        .expect("spawn");

        worker.stop();

        assert_eq!(
            *names.lock().expect("lock"),
            vec![Some("named-worker".to_string())]
        );
    }

    #[test]
    fn stop_does_not_fail_when_the_body_panicked() {
        let runs = Arc::new(AtomicUsize::new(0));
        let thread_runs = Arc::clone(&runs);
        let worker = Worker::spawn("panicking-worker", move |_| {
            thread_runs.fetch_add(1, Ordering::SeqCst);
            panic!("expected panic in test worker");
        })
        .expect("spawn");

        worker.stop();

        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn is_stopped_reflects_the_flag() {
        let flag = AtomicBool::new(false);
        assert!(!is_stopped(&flag));

        flag.store(true, Ordering::SeqCst);

        assert!(is_stopped(&flag));
    }
}
