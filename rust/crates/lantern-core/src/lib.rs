//! Experimental simulation core boundary. Gameplay rules are not ported yet.
//!
//! Future rules receive time, commands, and random inputs explicitly from the host.
//! Networking, filesystem access, and wall-clock access belong outside this crate.

#![no_std]

extern crate alloc;

pub mod ids;
pub mod legacy_map;
pub mod model;
pub mod python_random;
pub mod seed_material;
pub mod spatial;
pub mod starter;
pub mod terrain;
pub mod units;
