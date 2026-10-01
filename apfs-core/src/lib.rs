// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![cfg_attr(not(feature = "std"), no_std)]

//! APFS reading and writing primitives.
//!
//! Without the default `std` feature this crate is `no_std` + `alloc` and
//! builds on stable Rust. I/O uses [embedded_io] traits (see [io]).

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod block;
pub mod btree;
pub mod container {
    pub use apfs_types::container::*;
}
pub mod data_stream {
    pub use apfs_types::data_stream::*;
}
pub mod encryption {
    pub use apfs_types::encryption::*;
}
pub mod error;
pub mod filesystem;
pub mod io;
pub mod filesystem_extended_fields {
    pub use apfs_types::filesystem_extended_fields::*;
}
pub mod object {
    pub use apfs_types::object::*;
}
pub mod object_map;
pub mod read;
pub mod reaper {
    pub use apfs_types::reaper::*;
}
pub mod sealed_volume {
    pub use apfs_types::sealed_volume::*;
}
pub mod sibling {
    pub use apfs_types::sibling::*;
}
pub mod snapshot {
    pub use apfs_types::snapshot::*;
}
pub mod space_manager;
pub mod write;

pub use apfs_types::common;
pub use apfs_types::{ParseError, ParsedDiskStruct};
