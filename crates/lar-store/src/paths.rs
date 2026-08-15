use std::env;
use std::path::PathBuf;

/// Overrides the user prefix (`~/.local/share/lar`).
pub const LAR_USER_PREFIX_ENV: &str = "LAR_USER_PREFIX";

/// Overrides the system prefix (`/var/lib/lar`).
pub const LAR_SYSTEM_PREFIX_ENV: &str = "LAR_SYSTEM_PREFIX";

/// Resolved LAR paths for a prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub prefix: PathBuf,
    pub store: PathBuf,
    pub packages: PathBuf,
    pub system: bool,
}

impl Paths {
    /// Build paths for `prefix` (already resolved).
    pub fn from_prefix(prefix: PathBuf, system: bool) -> Self {
        let store = prefix.join("store");
        let packages = store.join("packages");
        Self {
            prefix,
            store,
            packages,
            system,
        }
    }
}

/// Resolve the LAR prefix.
///
/// - System mode: `LAR_SYSTEM_PREFIX` if set, else `/var/lib/lar`
/// - User mode: `LAR_USER_PREFIX` if set, else `~/.local/share/lar`
pub fn prefix(system: bool) -> PathBuf {
    if system {
        if let Ok(override_prefix) = env::var(LAR_SYSTEM_PREFIX_ENV) {
            if !override_prefix.is_empty() {
                return PathBuf::from(override_prefix);
            }
        }
        PathBuf::from("/var/lib/lar")
    } else {
        if let Ok(override_prefix) = env::var(LAR_USER_PREFIX_ENV) {
            if !override_prefix.is_empty() {
                return PathBuf::from(override_prefix);
            }
        }
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/share/lar")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_from_prefix_layout() {
        let paths = Paths::from_prefix(PathBuf::from("/tmp/lar-test"), false);
        assert_eq!(paths.store, PathBuf::from("/tmp/lar-test/store"));
        assert_eq!(
            paths.packages,
            PathBuf::from("/tmp/lar-test/store/packages")
        );
    }
}
