//! Package sources (repos): config, fetch, signatures, and advisories.

mod advisories;
mod audit;
mod error;
mod fetch;
mod index;
mod publish;
mod sources;
mod transport;
mod trust;
mod versions;

pub use advisories::{
    empty_advisories, parse_advisories, sign_advisories, sign_advisories_in_dir, verify_advisories,
    write_advisories, AdvisoriesFile, Advisory, Severity,
};
pub use audit::{audit, audit_should_fail, AuditFinding, AuditScope};
pub use error::Error;
pub use fetch::{
    collect_warnings_for_pin, emit_store_hit_warnings, ensure_package, fetch_into_store,
    load_package_for_resolve, AdvisoryWarning, ResolvePackage,
};
pub use index::{
    build_index, index_pin_signing_message, parse_index, sign_index_package, verify_index_package,
    write_index, IndexPackage, PackageIndex, INDEX_FORMAT,
};
pub use publish::{
    init_repo, publish_package, unpublish_package, validate_repo, write_repo_pubkey,
    write_repo_pubkey_from_secret, IndexPackageInfo, ValidateReport,
};
pub use sources::{
    add_source, default_source_name, load_sources, move_source, move_source_after,
    move_source_before, ordered_sources, remove_source, save_sources, SourceEntry, SourcesFile,
    SOURCES_FORMAT,
};
pub use transport::{parse_uri, read_advisories, read_index, read_repo_pubkey, SourceBase, REPO_PUBKEY_FILE};
pub use trust::{
    fingerprint_matches, is_key_trusted, key_id_from_public, keygen, load_source_pubkey, load_trust,
    save_trust, sign_content_hash, sign_message, trust_add, trust_remove, verify_content_hash,
    verify_message, TrustFile, TrustedKey, TRUST_FORMAT,
};
pub use versions::{list_dep_versions, list_yanked_dep_versions, YankedDepVersion};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;
