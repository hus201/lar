//! Local immutable Side-by-Side package store.

mod error;
mod paths;
mod store;

pub use error::Error;
pub use paths::{prefix, Paths, LAR_SYSTEM_PREFIX_ENV, LAR_USER_PREFIX_ENV};
pub use store::{Store, StoredPackage};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
