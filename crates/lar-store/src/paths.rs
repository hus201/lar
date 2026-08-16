use std::env;
use std::path::PathBuf;

/// Overrides the user prefix (`~/.local/share/lar`).
pub const LAR_USER_PREFIX_ENV: &str = "LAR_USER_PREFIX";

/// Resolved LAR paths for a prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub prefix: PathBuf,
    pub store: PathBuf,
    pub packages: PathBuf,
    pub runtimes: PathBuf,
    pub installs: PathBuf,
    pub config: PathBuf,
    /// Session applications directory for published `.desktop` copies.
    pub applications: PathBuf,
    /// Session bin directory for PATH command shims.
    pub bin: PathBuf,
    pub system: bool,
}

impl Paths {
    /// Build paths for `prefix` (already resolved).
    pub fn from_prefix(prefix: PathBuf, system: bool) -> Self {
        Self::from_prefix_with_exports(
            prefix,
            system,
            resolve_applications_dir(system),
            resolve_bin_dir(system),
        )
    }

    /// Like [`from_prefix`], but with explicit session applications and bin directories.
    pub fn from_prefix_with_exports(
        prefix: PathBuf,
        system: bool,
        applications: PathBuf,
        bin: PathBuf,
    ) -> Self {
        let store = prefix.join("store");
        let packages = store.join("packages");
        let runtimes = prefix.join("runtimes");
        let installs = prefix.join("installs");
        let config = prefix.join("config");
        Self {
            prefix,
            store,
            packages,
            runtimes,
            installs,
            config,
            applications,
            bin,
            system,
        }
    }

    /// Like [`from_prefix`], but with an explicit session applications directory.
    /// Session bin uses [`resolve_bin_dir`].
    pub fn from_prefix_with_applications(
        prefix: PathBuf,
        system: bool,
        applications: PathBuf,
    ) -> Self {
        Self::from_prefix_with_exports(prefix, system, applications, resolve_bin_dir(system))
    }

    /// Path to configured package sources.
    pub fn sources_toml(&self) -> PathBuf {
        self.config.join("sources.toml")
    }

    /// Path to trusted publisher public keys.
    pub fn trust_toml(&self) -> PathBuf {
        self.config.join("trust.toml")
    }

    /// LAR-owned applications directory for `.desktop` files.
    pub fn share_applications(&self) -> PathBuf {
        self.prefix.join("share").join("applications")
    }

    /// LAR-owned bin directory for PATH command shims.
    pub fn share_bin(&self) -> PathBuf {
        self.prefix.join("bin")
    }

    /// Metadata for PATH exports (`{cmd}.toml`), used by the native trampoline.
    pub fn share_exports(&self) -> PathBuf {
        self.prefix.join("share").join("lar").join("exports")
    }

    /// Stable `lar` CLI symlink refreshed on publish/launch (`{prefix}/libexec/lar`).
    pub fn libexec_lar(&self) -> PathBuf {
        self.prefix.join("libexec").join("lar")
    }
}

/// Session applications dir for desktop publish (XDG / system).
pub fn resolve_applications_dir(system: bool) -> PathBuf {
    if system {
        PathBuf::from("/usr/local/share/applications")
    } else if let Ok(xdg) = env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("applications");
        }
        user_home_applications()
    } else {
        user_home_applications()
    }
}

/// Session bin dir for PATH exports.
pub fn resolve_bin_dir(system: bool) -> PathBuf {
    if system {
        PathBuf::from("/usr/local/bin")
    } else {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/bin")
    }
}

fn user_home_applications() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/applications")
}

/// Resolve the LAR prefix.
///
/// - System mode: always `/var/lib/lar`
/// - User mode: `LAR_USER_PREFIX` if set, else `~/.local/share/lar`
pub fn prefix(system: bool) -> PathBuf {
    if system {
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
        assert_eq!(paths.runtimes, PathBuf::from("/tmp/lar-test/runtimes"));
        assert_eq!(paths.installs, PathBuf::from("/tmp/lar-test/installs"));
        assert_eq!(paths.config, PathBuf::from("/tmp/lar-test/config"));
        assert_eq!(
            paths.sources_toml(),
            PathBuf::from("/tmp/lar-test/config/sources.toml")
        );
        assert_eq!(
            paths.trust_toml(),
            PathBuf::from("/tmp/lar-test/config/trust.toml")
        );
        assert_eq!(
            paths.share_applications(),
            PathBuf::from("/tmp/lar-test/share/applications")
        );
        assert_eq!(paths.share_bin(), PathBuf::from("/tmp/lar-test/bin"));
        assert_eq!(
            paths.share_exports(),
            PathBuf::from("/tmp/lar-test/share/lar/exports")
        );
        assert_eq!(
            paths.libexec_lar(),
            PathBuf::from("/tmp/lar-test/libexec/lar")
        );
        assert!(!paths.applications.as_os_str().is_empty());
        assert!(!paths.bin.as_os_str().is_empty());
    }

    #[test]
    fn system_prefix_is_fixed() {
        assert_eq!(prefix(true), PathBuf::from("/var/lib/lar"));
        assert_eq!(resolve_bin_dir(true), PathBuf::from("/usr/local/bin"));
    }
}
