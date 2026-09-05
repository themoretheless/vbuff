//! Bounded command queue owning the only writable database connection.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use vbuff_store::Store;

type Job = Box<dyn FnOnce(&Store) + Send>;
pub(crate) struct StoreOwner {
    sender: Mutex<Option<mpsc::SyncSender<Job>>>,
    pending: Arc<AtomicUsize>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl StoreOwner {
    pub(crate) fn new(store: Store) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Job>(32);
        let pending = Arc::new(AtomicUsize::new(0));
        let worker = std::thread::Builder::new()
            .name("vbuff-store".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job(&store);
                }
                if let Err(error) = store.scrub_wal_if_dirty() {
                    tracing::warn!("history checkpoint failed: {error}");
                }
            })
            .expect("spawn history owner");
        Self {
            sender: Mutex::new(Some(sender)),
            pending,
            worker: Mutex::new(Some(worker)),
        }
    }
    pub(crate) fn execute<T: Send + 'static>(
        &self,
        task: impl FnOnce(&Store) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<T> {
        self.pending.fetch_add(1, Ordering::AcqRel);
        self.submit(task)
    }
    pub(crate) fn try_execute<T: Send + 'static>(
        &self,
        task: impl FnOnce(&Store) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<Option<T>> {
        if self
            .pending
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(None);
        }
        self.submit(task).map(Some)
    }
    fn submit<T: Send + 'static>(
        &self,
        task: impl FnOnce(&Store) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<T> {
        let sender = self
            .sender
            .lock()
            .map_err(|_| anyhow::anyhow!("history owner poisoned"))?
            .clone();
        let Some(sender) = sender else {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            anyhow::bail!("history owner stopped");
        };
        let (reply, result) = mpsc::sync_channel(1);
        let pending = self.pending.clone();
        if sender
            .send(Box::new(move |store| {
                let value = task(store);
                pending.fetch_sub(1, Ordering::AcqRel);
                let _ = reply.send(value);
            }))
            .is_err()
        {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            anyhow::bail!("history owner unavailable");
        }
        result
            .recv()
            .map_err(|_| anyhow::anyhow!("history owner stopped before replying"))?
    }
    pub(crate) fn shutdown(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        if let Ok(mut worker) = self.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
    #[cfg(test)]
    pub(crate) fn hold_busy(&self) -> BusyGuard {
        self.pending.fetch_add(1, Ordering::AcqRel);
        BusyGuard(self.pending.clone())
    }
}
impl Drop for StoreOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[cfg(test)]
pub(crate) struct BusyGuard(Arc<AtomicUsize>);
#[cfg(test)]
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owns_one_thread_and_shutdown_rejects_new_work() {
        let owner = StoreOwner::new(Store::open_in_memory().unwrap());
        let thread = owner.execute(|_| Ok(std::thread::current().id())).unwrap();
        assert_ne!(thread, std::thread::current().id());
        assert_eq!(
            owner.execute(|_| Ok(std::thread::current().id())).unwrap(),
            thread
        );
        owner.shutdown();
        assert!(owner.execute(|store| Ok(store.count()?)).is_err());
    }
    #[test]
    fn maintenance_skips_busy_owner_and_recovers_after_completion() {
        let owner = Arc::new(StoreOwner::new(Store::open_in_memory().unwrap()));
        let (entered, started) = mpsc::sync_channel(0);
        let (release, wait) = mpsc::sync_channel(0);
        let busy = owner.clone();
        let worker = std::thread::spawn(move || {
            busy.execute(move |_| {
                entered.send(())?;
                wait.recv()?;
                Ok(())
            })
        });
        started.recv().unwrap();
        assert!(
            owner
                .try_execute(|store| Ok(store.count()?))
                .unwrap()
                .is_none()
        );
        release.send(()).unwrap();
        worker.join().unwrap().unwrap();
        assert_eq!(
            owner.try_execute(|store| Ok(store.count()?)).unwrap(),
            Some(0)
        );
    }
}
