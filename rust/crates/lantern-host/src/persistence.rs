//! Experimental private generation storage. No Python conversion or HTTP wiring.
//! Capture under one simulation owner; submit and wait outside that owner.
mod snapshot;
mod store;
mod worker;
pub use snapshot::{
    RegistrySnapshot, Resources, RestoredModel, RestoredRegistry, RestoredWorld, TownState,
    WorldSnapshot,
};
pub use store::{Generation, Recovery, Store};
pub use worker::{SaveService, SaveTicket};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Io(std::io::ErrorKind),
    Invalid(&'static str),
    Busy,
    Closed,
    Worker,
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}
type Result<T> = std::result::Result<T, Error>;
fn invalid(reason: &'static str) -> Error {
    Error::Invalid(reason)
}

#[cfg(test)]
mod tests;
