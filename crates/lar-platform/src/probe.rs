use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lar_package::PLATFORM_CAPABILITIES;

use crate::Error;
use crate::Result;
use crate::OVERRIDE_ENV;

/// Built-in host capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Wayland,
    X11,
    Vulkan,
    OpenGl,
    Dbus,
    Dri,
    SystemdUser,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
            Self::Vulkan => "vulkan",
            Self::OpenGl => "opengl",
            Self::Dbus => "dbus",
            Self::Dri => "dri",
            Self::SystemdUser => "systemd-user",
        }
    }

    pub fn all() -> &'static [Capability] {
        &[
            Self::Wayland,
            Self::X11,
            Self::Vulkan,
            Self::OpenGl,
            Self::Dbus,
            Self::Dri,
            Self::SystemdUser,
        ]
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parse a capability id string.
pub fn parse_capability(id: &str) -> Result<Capability> {
    match id {
        "wayland" => Ok(Capability::Wayland),
        "x11" => Ok(Capability::X11),
        "vulkan" => Ok(Capability::Vulkan),
        "opengl" => Ok(Capability::OpenGl),
        "dbus" => Ok(Capability::Dbus),
        "dri" => Ok(Capability::Dri),
        "systemd-user" => Ok(Capability::SystemdUser),
        other => {
            debug_assert!(
                !PLATFORM_CAPABILITIES.contains(&other),
                "PLATFORM_CAPABILITIES out of sync with Capability"
            );
            Err(Error::UnknownCapability(other.to_string()))
        }
    }
}

/// Result of probing the host for one capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub capability: Capability,
    pub present: bool,
    pub detail: String,
}

/// Environment inputs for probes (overridable in tests).
#[derive(Debug, Clone)]
pub struct ProbeEnv {
    pub wayland_display: Option<String>,
    pub display: Option<String>,
    pub dbus_session_bus_address: Option<String>,
    pub xdg_runtime_dir: Option<PathBuf>,
    pub uid: u32,
    pub ld_library_path: Option<String>,
    pub dri_dir: PathBuf,
    pub run_systemd_system: PathBuf,
    /// When set, `missing=a,b` / `present=c` force probe outcomes.
    pub override_spec: Option<String>,
}

impl ProbeEnv {
    pub fn from_process() -> Self {
        Self {
            wayland_display: env::var("WAYLAND_DISPLAY").ok().filter(|s| !s.is_empty()),
            display: env::var("DISPLAY").ok().filter(|s| !s.is_empty()),
            dbus_session_bus_address: env::var("DBUS_SESSION_BUS_ADDRESS")
                .ok()
                .filter(|s| !s.is_empty()),
            xdg_runtime_dir: env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            uid: {
                #[cfg(unix)]
                {
                    libc_uid()
                }
                #[cfg(not(unix))]
                {
                    0
                }
            },
            ld_library_path: env::var("LD_LIBRARY_PATH").ok(),
            dri_dir: PathBuf::from("/dev/dri"),
            run_systemd_system: PathBuf::from("/run/systemd/system"),
            override_spec: env::var(OVERRIDE_ENV).ok(),
        }
    }
}

#[cfg(unix)]
fn libc_uid() -> u32 {
    // Avoid libc crate dep: read from /proc/self/status or use nix-free approach.
    // std doesn't expose getuid; parse /proc/self/loginuid or use whoami via nix.
    // Simplest portable-enough: parse `id -u` is heavy; use /proc/self/status Uid.
    if let Ok(text) = fs::read_to_string("/proc/self/status") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                if let Some(uid) = rest.split_whitespace().next() {
                    if let Ok(n) = uid.parse() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

/// Probe one capability using process environment.
pub fn probe(cap: Capability) -> ProbeResult {
    probe_with(cap, &ProbeEnv::from_process())
}

/// Probe with an explicit environment (tests / overrides).
pub fn probe_with(cap: Capability, env: &ProbeEnv) -> ProbeResult {
    if let Some(forced) = override_force(env, cap) {
        return ProbeResult {
            capability: cap,
            present: forced,
            detail: format!("forced by {OVERRIDE_ENV}"),
        };
    }
    match cap {
        Capability::Wayland => probe_wayland(env),
        Capability::X11 => probe_x11(env),
        Capability::Vulkan => probe_lib(env, Capability::Vulkan, &["libvulkan.so.1"]),
        Capability::OpenGl => probe_lib(env, Capability::OpenGl, &["libGL.so.1", "libOpenGL.so.0"]),
        Capability::Dbus => probe_dbus(env),
        Capability::Dri => probe_dri(env),
        Capability::SystemdUser => probe_systemd_user(env),
    }
}

fn override_force(env: &ProbeEnv, cap: Capability) -> Option<bool> {
    let spec = env.override_spec.as_deref()?;
    let id = cap.as_str();
    for part in spec.split(',') {
        let part = part.trim();
        if let Some(list) = part.strip_prefix("missing=") {
            if list.split('+').any(|s| s.trim() == id) {
                return Some(false);
            }
        }
        if let Some(list) = part.strip_prefix("present=") {
            if list.split('+').any(|s| s.trim() == id) {
                return Some(true);
            }
        }
    }
    None
}

fn probe_wayland(env: &ProbeEnv) -> ProbeResult {
    if let Some(ref display) = env.wayland_display {
        return ProbeResult {
            capability: Capability::Wayland,
            present: true,
            detail: format!("WAYLAND_DISPLAY={display}"),
        };
    }
    let runtime = env
        .xdg_runtime_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", env.uid)));
    if let Ok(entries) = fs::read_dir(&runtime) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("wayland-") {
                return ProbeResult {
                    capability: Capability::Wayland,
                    present: true,
                    detail: format!("found {}/{}", runtime.display(), name),
                };
            }
        }
    }
    ProbeResult {
        capability: Capability::Wayland,
        present: false,
        detail: format!(
            "WAYLAND_DISPLAY unset and no wayland-* under {}",
            runtime.display()
        ),
    }
}

fn probe_x11(env: &ProbeEnv) -> ProbeResult {
    if let Some(ref display) = env.display {
        ProbeResult {
            capability: Capability::X11,
            present: true,
            detail: format!("DISPLAY={display}"),
        }
    } else {
        ProbeResult {
            capability: Capability::X11,
            present: false,
            detail: "DISPLAY unset".into(),
        }
    }
}

fn probe_dbus(env: &ProbeEnv) -> ProbeResult {
    if let Some(ref addr) = env.dbus_session_bus_address {
        return ProbeResult {
            capability: Capability::Dbus,
            present: true,
            detail: format!("DBUS_SESSION_BUS_ADDRESS set ({addr})"),
        };
    }
    let runtime = env
        .xdg_runtime_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", env.uid)));
    let bus = runtime.join("bus");
    if bus.exists() {
        return ProbeResult {
            capability: Capability::Dbus,
            present: true,
            detail: format!("found {}", bus.display()),
        };
    }
    ProbeResult {
        capability: Capability::Dbus,
        present: false,
        detail: format!(
            "DBUS_SESSION_BUS_ADDRESS unset and {} missing",
            bus.display()
        ),
    }
}

fn probe_dri(env: &ProbeEnv) -> ProbeResult {
    let dir = &env.dri_dir;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("card") || name.starts_with("renderD") {
                return ProbeResult {
                    capability: Capability::Dri,
                    present: true,
                    detail: format!("found {}/{}", dir.display(), name),
                };
            }
        }
    }
    ProbeResult {
        capability: Capability::Dri,
        present: false,
        detail: format!("no card*/renderD* under {}", dir.display()),
    }
}

fn probe_lib(env: &ProbeEnv, cap: Capability, names: &[&str]) -> ProbeResult {
    let mut dirs = Vec::new();
    if let Some(ref ld) = env.ld_library_path {
        for part in ld.split(':') {
            if !part.is_empty() {
                dirs.push(PathBuf::from(part));
            }
        }
    }
    for d in [
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/lib",
        "/lib64",
        "/lib/x86_64-linux-gnu",
    ] {
        dirs.push(PathBuf::from(d));
    }
    for dir in &dirs {
        for name in names {
            let path = dir.join(name);
            if path.is_file() || path_is_symlink_file(&path) {
                return ProbeResult {
                    capability: cap,
                    present: true,
                    detail: format!("found {}", path.display()),
                };
            }
        }
    }
    ProbeResult {
        capability: cap,
        present: false,
        detail: format!("none of {} found on library path", names.join(", ")),
    }
}

fn path_is_symlink_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink() || m.is_file())
        .unwrap_or(false)
}

fn probe_systemd_user(env: &ProbeEnv) -> ProbeResult {
    if !env.run_systemd_system.exists() {
        return ProbeResult {
            capability: Capability::SystemdUser,
            present: false,
            detail: format!("{} missing", env.run_systemd_system.display()),
        };
    }
    match Command::new("systemctl")
        .args(["--user", "is-system-running"])
        .output()
    {
        Ok(out) if out.status.success() => ProbeResult {
            capability: Capability::SystemdUser,
            present: true,
            detail: "systemctl --user is-system-running ok".into(),
        },
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let status = stdout.trim();
            // "running", "degraded", "maintenance" still mean a user instance exists.
            if matches!(status, "running" | "degraded" | "maintenance") {
                return ProbeResult {
                    capability: Capability::SystemdUser,
                    present: true,
                    detail: format!("systemctl --user status={status}"),
                };
            }
            ProbeResult {
                capability: Capability::SystemdUser,
                present: false,
                detail: format!("systemctl --user is-system-running failed ({status} {stderr})"),
            }
        }
        Err(err) => {
            // Fallback: user runtime bus often implies a session with systemd.
            let runtime = env
                .xdg_runtime_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", env.uid)));
            if runtime.join("systemd").is_dir() {
                ProbeResult {
                    capability: Capability::SystemdUser,
                    present: true,
                    detail: format!(
                        "systemctl unavailable ({err}); found {}/systemd",
                        runtime.display()
                    ),
                }
            } else {
                ProbeResult {
                    capability: Capability::SystemdUser,
                    present: false,
                    detail: format!("systemctl unavailable ({err}) and no user systemd runtime"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_env() -> ProbeEnv {
        ProbeEnv {
            wayland_display: None,
            display: None,
            dbus_session_bus_address: None,
            xdg_runtime_dir: None,
            uid: 99999,
            ld_library_path: None,
            dri_dir: PathBuf::from("/tmp/lar-no-dri"),
            run_systemd_system: PathBuf::from("/tmp/lar-no-systemd"),
            override_spec: None,
        }
    }

    #[test]
    fn override_forces_missing_and_present() {
        let mut env = empty_env();
        env.override_spec = Some("missing=wayland+x11,present=dbus".into());
        assert!(!probe_with(Capability::Wayland, &env).present);
        assert!(!probe_with(Capability::X11, &env).present);
        assert!(probe_with(Capability::Dbus, &env).present);
    }

    #[test]
    fn x11_from_display() {
        let mut env = empty_env();
        env.display = Some(":0".into());
        let r = probe_with(Capability::X11, &env);
        assert!(r.present);
        assert!(r.detail.contains("DISPLAY"));
    }

    #[test]
    fn wayland_from_runtime_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::File::create(dir.path().join("wayland-0")).unwrap();
        let mut env = empty_env();
        env.xdg_runtime_dir = Some(dir.path().to_path_buf());
        assert!(probe_with(Capability::Wayland, &env).present);
    }
}
