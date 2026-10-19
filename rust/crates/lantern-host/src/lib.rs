//! Experimental offline model boundaries and legacy staging import. No HTTP support.
//!
//! Authoritative state is deliberately not serializable:
//! ```compile_fail
//! use lantern_host::state::ModelWorld;
//! fn accidental_snapshot(world: &ModelWorld) { serde_json::to_string(world).unwrap(); }
//! ```
//! ```compile_fail
//! use lantern_core::model::Town;
//! fn accidental_town(town: &Town) { serde_json::to_string(town).unwrap(); }
//! ```
//! ```compile_fail
//! use lantern_host::state::PrivateCredentials;
//! serde_json::to_string(&PrivateCredentials::default()).unwrap();
//! ```

pub mod building;
pub mod dto;
pub mod farming;
pub mod fishing;
pub mod gathering;
pub mod legacy;
pub mod livestock;
pub mod logistics;
pub mod navigation;
pub mod orchards;
pub mod population;
pub mod processing;
pub mod progression;
pub mod references;
pub mod resources;
pub mod social;
pub mod state;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Invalid(&'static str),
    Json { line: usize, column: usize },
    Identity(lantern_core::ids::IdentityError),
    Model(lantern_core::model::ModelError),
    Unit(lantern_core::units::UnitError),
}

/// Diagnostics contain schema paths and categories, never input values/secrets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryError {
    pub path: String,
    pub kind: ErrorKind,
}

impl BoundaryError {
    pub(crate) fn at(path: impl Into<String>, kind: ErrorKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }
    pub(crate) fn invalid(path: impl Into<String>, reason: &'static str) -> Self {
        Self::at(path, ErrorKind::Invalid(reason))
    }
}
