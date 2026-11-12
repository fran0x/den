//! Small cancellable worker shared by filesystem and Homebrew collection.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
};

pub struct Worker<T, E> {
    receiver: Receiver<Result<T, E>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    join_on_drop: bool,
}
impl<T: Send + 'static, E: From<std::io::Error> + Send + 'static> Worker<T, E> {
    pub(crate) fn start_with(
        run: impl FnOnce(&AtomicBool) -> Result<T, E> + Send + 'static,
    ) -> Result<Self, E> {
        let (sender, receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker = thread::Builder::new()
            .name("den-collector".into())
            .spawn(move || {
                let _ = sender.send(run(&worker_cancel));
            })
            .map_err(E::from)?;
        Ok(Self {
            receiver,
            cancel,
            worker: Some(worker),
            join_on_drop: true,
        })
    }
    /// Filesystem reads have no subprocess to reap and an OS call may block.
    /// Cancel cooperatively, but permit terminal restoration/exit without waiting.
    /// Homebrew workers retain the default join so children are killed and reaped.
    pub(crate) fn detach_on_drop(mut self) -> Self {
        self.join_on_drop = false;
        self
    }
    /// Asks the work to stop without waiting for it; the result still arrives by `poll`.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn poll(&self) -> Option<Result<T, E>> {
        match self.receiver.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(E::from(std::io::Error::other(
                "collection worker stopped",
            )))),
        }
    }
}
impl<T, E> Drop for Worker<T, E> {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take()
            && self.join_on_drop
        {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_blocked_read_only_worker_does_not_delay_terminal_exit() {
        let (started, ready) = mpsc::channel();
        let worker = Worker::<(), std::io::Error>::start_with(move |_| {
            started.send(()).unwrap();
            thread::sleep(std::time::Duration::from_secs(2));
            Ok(())
        })
        .unwrap()
        .detach_on_drop();
        ready.recv().unwrap();
        let start = std::time::Instant::now();
        drop(worker);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }
}
