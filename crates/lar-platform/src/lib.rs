//! Host platform capability probes (OS/LAR boundary).

mod check;
mod error;
mod probe;

pub use check::{
    check_host, check_host_with, collect_from_manifests, collect_platform_need, ensure_host,
    CheckReport, PlatformNeed,
};
pub use error::Error;
pub use probe::{parse_capability, probe, probe_with, Capability, ProbeEnv, ProbeResult};

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Test/helper override: `LAR_PLATFORM_OVERRIDE=missing=wayland,present=x11`
pub const OVERRIDE_ENV: &str = "LAR_PLATFORM_OVERRIDE";
