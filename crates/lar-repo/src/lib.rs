//! Package sources (repos): config, fetch, signatures, and advisories.

mod advisories;
mod audit;
mod error;
mod fetch;
mod index;
mod policy;
mod sources;
mod transport;
mod trust;

pub use advisories::{empty_advisories, parse_advisories, AdvisoriesFile, Advisory, Severity};
pub use audit::{audit, audit_should_fail, AuditFinding, AuditScope};
pub use error::Error;
pub use fetch::{
    collect_warnings_for_pin, emit_store_hit_warnings, ensure_package, fetch_into_store,
    AdvisoryWarning,
};
pub use index::{build_index, parse_index, write_index, IndexPackage, PackageIndex, INDEX_FORMAT};
pub use policy::{LookupMode, SourcePolicy};
pub use sources::{
    add_source, default_source_name, load_sources, ordered_apps_sources, ordered_deps_sources,
    remove_source, save_sources, SourceEntry, SourcesFile, SOURCES_FORMAT,
};
pub use transport::{parse_uri, read_advisories, read_index, SourceBase};
pub use trust::{
    key_id_from_public, keygen, load_trust, save_trust, sign_content_hash, trust_add, trust_remove,
    verify_content_hash, TrustFile, TrustedKey, TRUST_FORMAT,
};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
