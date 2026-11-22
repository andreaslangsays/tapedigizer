use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};

#[derive(Default)]
pub(super) struct ExportProgress {
    cancelled: AtomicBool,
    finished: AtomicUsize,
    total: AtomicUsize,
}

impl ExportProgress {
    pub fn report(&self, finished: usize, total: usize) -> bool {
        self.total.store(total, Ordering::Relaxed);
        self.finished.store(finished, Ordering::Relaxed);
        !self.is_cancelled()
    }

    pub fn counts(&self) -> (usize, usize) {
        (
            self.finished.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

pub(super) struct PendingJob<T> {
    pub label: &'static str,
    pub export_progress: Option<Arc<ExportProgress>>,
    result: Receiver<Result<T, String>>,
}

impl<T: Send + 'static> PendingJob<T> {
    pub fn start(
        ctx: &egui::Context,
        label: &'static str,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> std::io::Result<Self> {
        let ctx = ctx.clone();
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("tapedigizer-job".into())
            .spawn(move || {
                // Drop the sender before waking the UI, including after a panic.
                // The receiver then reports a disconnected worker instead of
                // leaving an idle window stuck in its busy state.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
                if let Ok(result) = outcome {
                    let _ = sender.send(result);
                }
                drop(sender);
                ctx.request_repaint();
            })?;
        Ok(Self {
            label,
            export_progress: None,
            result,
        })
    }

    pub fn poll(&self, interrupted: &'static str) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(interrupted.into())),
        }
    }

    #[cfg(test)]
    pub fn completed(label: &'static str, value: T) -> Self {
        let (sender, result) = mpsc::sync_channel(1);
        sender.send(Ok(value)).ok().expect("test receiver exists");
        Self {
            label,
            export_progress: None,
            result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_worker_wakes_the_ui_and_finishes_the_job() {
        let ctx = egui::Context::default();
        let (woken, wake) = mpsc::channel();
        ctx.set_request_repaint_callback(move |_| {
            let _ = woken.send(());
        });
        let job = PendingJob::<()>::start(&ctx, "test", || panic!("test worker failure")).unwrap();
        wake.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(job.poll("interrupted"), Some(Err("interrupted".into())));
    }
}
