//! Explicit, versioned, offline R05 model DTOs. No generic extension bags.
//! Public observations are not the browser's /api/world response. Saves reject
//! unsupported fields; R06 imports Python and R19 adds durable storage later.

mod facts;
mod public;
mod save;
mod wire;

pub use facts::EntityIdDto;
pub use public::{PublicWorldDto, public_snapshot};
pub use save::{decode_save, encode_save};
