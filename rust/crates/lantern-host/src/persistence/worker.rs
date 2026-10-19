use super::{Error, Generation, RegistrySnapshot, Result, Store};
use std::{
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

enum Request {
    Save(RegistrySnapshot, mpsc::Sender<Result<Generation>>),
    Flush(mpsc::Sender<Result<()>>),
}
pub struct SaveTicket {
    result: Receiver<Result<Generation>>,
}
impl SaveTicket {
    /// Poll once per owner turn; None means still queued/writing. A terminal
    /// result is consumed, so discard the ticket after receiving Some or Err.
    pub fn try_result(&self) -> Result<Option<Generation>> {
        match self.result.try_recv() {
            Ok(result) => result.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Error::Closed),
        }
    }

    /// Wait outside the simulation owner. Success means files and directories
    /// were synced and the entire generation was published.
    pub fn wait(self) -> Result<Generation> {
        self.result.recv().map_err(|_| Error::Closed)?
    }
}
/// One background writer, one pending save slot. Overload never reports success.
/// Call flush/shutdown outside authority; submit never waits on disk or capacity.
pub struct SaveService {
    sender: Option<SyncSender<Request>>,
    worker: Option<JoinHandle<Result<()>>>,
}
impl SaveService {
    pub fn start(store: Store) -> Self {
        Self::start_inner(store, |store, snapshot| store.commit(snapshot))
    }
    pub(super) fn start_inner(
        mut store: Store,
        mut commit: impl FnMut(&mut Store, RegistrySnapshot) -> Result<Generation> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let mut failure = None;
            while let Ok(request) = receiver.recv() {
                match request {
                    Request::Save(snapshot, reply) => {
                        let result = commit(&mut store, snapshot);
                        if let Err(error) = &result
                            && failure.is_none()
                        {
                            failure = Some(error.clone());
                        }
                        let _ = reply.send(result);
                    }
                    Request::Flush(reply) => {
                        let _ = reply.send(failure.take().map_or(Ok(()), Err));
                    }
                }
            }
            failure.map_or(Ok(()), Err)
        });
        Self {
            sender: Some(sender),
            worker: Some(worker),
        }
    }
    pub fn submit(&self, snapshot: RegistrySnapshot) -> Result<SaveTicket> {
        let (reply, result) = mpsc::channel();
        self.sender
            .as_ref()
            .ok_or(Error::Closed)?
            .try_send(Request::Save(snapshot, reply))
            .map_err(|e| match e {
                TrySendError::Full(_) => Error::Busy,
                TrySendError::Disconnected(_) => Error::Closed,
            })?;
        Ok(SaveTicket { result })
    }
    /// FIFO barrier: all earlier accepted saves finished. Reports any failure
    /// since the previous barrier, even when its ticket was dropped.
    pub fn flush(&self) -> Result<()> {
        let (reply, result) = mpsc::channel();
        self.sender
            .as_ref()
            .ok_or(Error::Closed)?
            .send(Request::Flush(reply))
            .map_err(|_| Error::Closed)?;
        result.recv().map_err(|_| Error::Closed)?
    }
    /// Close admission, drain accepted work, join, and return unreported failures.
    pub fn shutdown(mut self) -> Result<()> {
        self.sender.take();
        self.worker
            .take()
            .ok_or(Error::Closed)?
            .join()
            .map_err(|_| Error::Worker)?
    }
}
// Dropping closes admission, but detaches the thread. Explicit shutdown is needed
// before process exit; an implicit destructor must not block a simulation owner.
