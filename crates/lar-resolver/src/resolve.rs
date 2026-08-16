use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::io;
use std::path::Path;

use lar_package::{load_manifest, resolve_manifest_path, PackageManifest};
use lar_repo::ResolvePackage;
use lar_store::Store;
use pubgrub::{
    resolve as pubgrub_resolve, DefaultStringReporter, Dependencies, DependencyProvider,
    PackageResolutionStatistics, PubGrubError, Ranges, Reporter,
};
use semver::{Comparator, Op, Version, VersionReq};

use crate::lockfile::{LockRoot, LockedPackage, Lockfile, LOCKFILE_FORMAT};
use crate::Error;
use crate::Result;

type SemverRanges = Ranges<Version>;

/// Resolve dependencies for a root `package.toml` against `store`.
pub fn resolve(manifest_path: &Path, store: &Store) -> Result<Lockfile> {
    let manifest_path = resolve_manifest_path(manifest_path)?;
    let root_manifest = load_manifest(&manifest_path)?;
    resolve_manifest(&root_manifest, store)
}

/// Resolve from an already-loaded root manifest.
///
/// Uses the PubGrub conflict-driven algorithm (preferring highest matching
/// versions). Candidate metadata comes from the store / package index without
/// adding failed probes to the store; only the winning set is materialized.
pub fn resolve_manifest(root: &PackageManifest, store: &Store) -> Result<Lockfile> {
    let root_version = Version::parse(&root.package.version).map_err(|err| {
        Error::Other(format!(
            "root version `{}` is not semver: {err}",
            root.package.version
        ))
    })?;

    let provider = LarProvider {
        store,
        root_id: root.package.id.clone(),
        root_version: root_version.clone(),
        root_deps: root.dependencies.clone(),
        cache: RefCell::new(HashMap::new()),
        versions: RefCell::new(HashMap::new()),
    };

    let solution = match pubgrub_resolve(&provider, root.package.id.clone(), root_version) {
        Ok(solution) => solution,
        Err(PubGrubError::NoSolution(mut tree)) => {
            tree.collapse_no_versions();
            return Err(Error::Unresolvable(DefaultStringReporter::report(&tree)));
        }
        Err(PubGrubError::ErrorChoosingVersion { package, source }) => {
            return Err(map_choose_version_error(&package, source));
        }
        Err(err) => return Err(Error::Other(err.to_string())),
    };

    let cache = provider.cache.into_inner();
    let mut packages = BTreeMap::new();
    for (id, version) in &solution {
        let version_str = version.to_string();
        if id == &root.package.id && version_str == root.package.version {
            packages.insert(
                (id.clone(), version_str.clone()),
                LockedPackage {
                    id: id.clone(),
                    version: version_str,
                    content_hash: root.package.content_hash.clone(),
                    dependencies: root.dependencies.clone(),
                },
            );
            continue;
        }
        let pkg = cache
            .get(&(id.clone(), version_str.clone()))
            .ok_or_else(|| {
                Error::Other(format!(
                    "internal error: missing resolve metadata for {id} {version_str}"
                ))
            })?;
        packages.insert(
            (id.clone(), version_str.clone()),
            LockedPackage {
                id: id.clone(),
                version: version_str,
                content_hash: Some(pkg.content_hash.clone()),
                dependencies: pkg.dependencies.clone(),
            },
        );
    }

    if let Some((id, version)) = detect_dependency_cycle(&packages) {
        return Err(Error::Cycle { id, version });
    }

    materialize(store, &packages)?;

    let mut locked_packages: Vec<LockedPackage> = packages.into_values().collect();
    locked_packages.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));

    let lock = Lockfile {
        format: LOCKFILE_FORMAT,
        root: LockRoot {
            id: root.package.id.clone(),
            version: root.package.version.clone(),
        },
        packages: locked_packages,
    };
    lock.validate()?;
    Ok(lock)
}

struct LarProvider<'a> {
    store: &'a Store,
    root_id: String,
    root_version: Version,
    root_deps: BTreeMap<String, String>,
    cache: RefCell<HashMap<(String, String), ResolvePackage>>,
    versions: RefCell<HashMap<String, Vec<Version>>>,
}

#[derive(Debug)]
enum ProviderError {
    Yanked {
        id: String,
        version: String,
        advisory: String,
    },
    OnlyYanked {
        id: String,
        pins: Vec<(String, String)>,
    },
    Other(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yanked {
                id,
                version,
                advisory,
            } => write!(f, "package {id} {version} is yanked ({advisory})"),
            Self::OnlyYanked { id, pins } => write!(
                f,
                "package {id} has only yanked versions matching the requirement: {}",
                format_yanked_pins(pins)
            ),
            Self::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<lar_repo::Error> for ProviderError {
    fn from(err: lar_repo::Error) -> Self {
        match err {
            lar_repo::Error::Yanked {
                id,
                version,
                advisory,
            } => Self::Yanked {
                id,
                version,
                advisory,
            },
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<String> for ProviderError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl<'a> LarProvider<'a> {
    fn available_versions(&self, id: &str) -> std::result::Result<Vec<Version>, ProviderError> {
        if let Some(cached) = self.versions.borrow().get(id) {
            return Ok(cached.clone());
        }
        let mut parsed = Vec::new();
        for ver_str in lar_repo::list_dep_versions(self.store, id).map_err(ProviderError::from)? {
            if let Ok(ver) = Version::parse(&ver_str) {
                parsed.push(ver);
            }
        }
        parsed.sort();
        self.versions
            .borrow_mut()
            .insert(id.to_string(), parsed.clone());
        Ok(parsed)
    }

    fn load_pkg(
        &self,
        id: &str,
        version: &Version,
    ) -> std::result::Result<ResolvePackage, ProviderError> {
        let version_str = version.to_string();
        let key = (id.to_string(), version_str.clone());
        if let Some(pkg) = self.cache.borrow().get(&key) {
            return Ok(pkg.clone());
        }
        let mut sink = io::sink();
        let pkg = lar_repo::load_package_for_resolve(self.store, id, &version_str, &mut sink)
            .map_err(ProviderError::from)?;
        self.cache.borrow_mut().insert(key, pkg.clone());
        Ok(pkg)
    }
}

impl<'a> DependencyProvider for LarProvider<'a> {
    type P = String;
    type V = Version;
    type VS = SemverRanges;
    type Priority = (u32, Reverse<usize>);
    type M = String;
    type Err = ProviderError;

    fn prioritize(
        &self,
        package: &Self::P,
        range: &Self::VS,
        package_statistics: &PackageResolutionStatistics,
    ) -> Self::Priority {
        let version_count = self
            .available_versions(package)
            .map(|versions| versions.iter().filter(|v| range.contains(*v)).count())
            .unwrap_or(0);
        if version_count == 0 {
            return (u32::MAX, Reverse(0));
        }
        (package_statistics.conflict_count(), Reverse(version_count))
    }

    fn choose_version(
        &self,
        package: &Self::P,
        range: &Self::VS,
    ) -> std::result::Result<Option<Self::V>, Self::Err> {
        if package == &self.root_id {
            return Ok(range
                .contains(&self.root_version)
                .then(|| self.root_version.clone()));
        }
        let versions = self.available_versions(package)?;
        if let Some(v) = versions.into_iter().rev().find(|v| range.contains(v)) {
            return Ok(Some(v));
        }

        // No usable candidate — if yanked pins would have matched, say so explicitly.
        if let Some(err) = yanked_reason_in_range(self.store, package, range)? {
            return Err(err);
        }
        Ok(None)
    }

    fn get_dependencies(
        &self,
        package: &Self::P,
        version: &Self::V,
    ) -> std::result::Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        if package == &self.root_id && version == &self.root_version {
            return Ok(Dependencies::Available(deps_to_constraints(
                &self.root_deps,
            )?));
        }

        let pkg = match self.load_pkg(package, version) {
            Ok(pkg) => pkg,
            Err(err) => {
                return Ok(Dependencies::Unavailable(err.to_string()));
            }
        };
        Ok(Dependencies::Available(deps_to_constraints(
            &pkg.dependencies,
        )?))
    }
}

fn map_choose_version_error(package: &str, source: ProviderError) -> Error {
    match source {
        ProviderError::Yanked {
            id,
            version,
            advisory,
        } => Error::Repo(lar_repo::Error::Yanked {
            id,
            version,
            advisory,
        }),
        ProviderError::OnlyYanked { id, pins } => Error::Unresolvable(format!(
            "package {id} has only yanked versions matching the requirement: {}",
            format_yanked_pins(&pins)
        )),
        ProviderError::Other(msg) => {
            Error::Other(format!("choosing a version for {package} failed: {msg}"))
        }
    }
}

fn format_yanked_pins(pins: &[(String, String)]) -> String {
    pins.iter()
        .map(|(version, advisory)| format!("{version} ({advisory})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn yanked_reason_in_range(
    store: &Store,
    package: &str,
    range: &SemverRanges,
) -> std::result::Result<Option<ProviderError>, ProviderError> {
    let yanked =
        lar_repo::list_yanked_dep_versions(store, package).map_err(ProviderError::from)?;
    let mut matched: Vec<_> = yanked
        .into_iter()
        .filter(|y| {
            Version::parse(&y.version)
                .map(|v| range.contains(&v))
                .unwrap_or(false)
        })
        .collect();
    if matched.is_empty() {
        return Ok(None);
    }
    matched.sort_by(|a, b| {
        Version::parse(&b.version)
            .unwrap_or_else(|_| Version::new(0, 0, 0))
            .cmp(&Version::parse(&a.version).unwrap_or_else(|_| Version::new(0, 0, 0)))
    });
    if matched.len() == 1 {
        let y = matched.pop().unwrap();
        return Ok(Some(ProviderError::Yanked {
            id: package.to_string(),
            version: y.version,
            advisory: y.advisory,
        }));
    }
    Ok(Some(ProviderError::OnlyYanked {
        id: package.to_string(),
        pins: matched
            .into_iter()
            .map(|y| (y.version, y.advisory))
            .collect(),
    }))
}

fn deps_to_constraints(
    deps: &BTreeMap<String, String>,
) -> std::result::Result<pubgrub::DependencyConstraints<String, SemverRanges>, ProviderError> {
    let mut out = pubgrub::DependencyConstraints::default();
    for (id, req_str) in deps {
        let req = VersionReq::parse(req_str).map_err(|err| {
            ProviderError::Other(format!(
                "invalid version requirement `{req_str}` for {id}: {err}"
            ))
        })?;
        out.insert(id.clone(), version_req_to_ranges(&req)?);
    }
    Ok(out)
}

fn version_req_to_ranges(req: &VersionReq) -> std::result::Result<SemverRanges, ProviderError> {
    if req.comparators.is_empty() {
        return Err(ProviderError::Other(
            "wildcard version requirements are not supported".into(),
        ));
    }
    let mut ranges = SemverRanges::full();
    for comparator in &req.comparators {
        ranges = ranges.intersection(&comparator_to_ranges(comparator)?);
    }
    Ok(ranges)
}

fn comparator_to_ranges(c: &Comparator) -> std::result::Result<SemverRanges, ProviderError> {
    let major = c.major;
    let minor = c.minor;
    let patch = c.patch;

    match c.op {
        Op::Exact => Ok(exact_range(major, minor, patch)),
        Op::Greater => {
            let v = version_from_parts(major, minor, patch);
            Ok(SemverRanges::strictly_higher_than(v))
        }
        Op::GreaterEq => {
            let v = version_from_parts(major, minor, patch);
            Ok(SemverRanges::higher_than(v))
        }
        Op::Less => {
            let v = version_from_parts(major, minor, patch);
            Ok(SemverRanges::strictly_lower_than(v))
        }
        Op::LessEq => {
            let v = version_from_parts(major, minor, patch);
            Ok(SemverRanges::lower_than(v))
        }
        Op::Tilde => Ok(tilde_range(major, minor, patch)),
        Op::Caret => Ok(caret_range(major, minor, patch)),
        Op::Wildcard => Ok(exact_range(major, minor, patch)),
        _ => Err(ProviderError::Other(
            "unsupported version comparator operator in requirement".into(),
        )),
    }
}

fn version_from_parts(major: u64, minor: Option<u64>, patch: Option<u64>) -> Version {
    Version::new(major, minor.unwrap_or(0), patch.unwrap_or(0))
}

fn exact_range(major: u64, minor: Option<u64>, patch: Option<u64>) -> SemverRanges {
    match (minor, patch) {
        (Some(mi), Some(pa)) => SemverRanges::singleton(Version::new(major, mi, pa)),
        (Some(mi), None) => {
            SemverRanges::between(Version::new(major, mi, 0), Version::new(major, mi + 1, 0))
        }
        (None, _) => {
            SemverRanges::between(Version::new(major, 0, 0), Version::new(major + 1, 0, 0))
        }
    }
}

fn tilde_range(major: u64, minor: Option<u64>, patch: Option<u64>) -> SemverRanges {
    match (minor, patch) {
        (Some(mi), Some(pa)) => {
            SemverRanges::between(Version::new(major, mi, pa), Version::new(major, mi + 1, 0))
        }
        (Some(mi), None) => {
            SemverRanges::between(Version::new(major, mi, 0), Version::new(major, mi + 1, 0))
        }
        (None, _) => {
            SemverRanges::between(Version::new(major, 0, 0), Version::new(major + 1, 0, 0))
        }
    }
}

fn caret_range(major: u64, minor: Option<u64>, patch: Option<u64>) -> SemverRanges {
    let mi = minor.unwrap_or(0);
    let pa = patch.unwrap_or(0);
    let start = Version::new(major, mi, pa);
    let end = if major > 0 {
        Version::new(major + 1, 0, 0)
    } else if mi > 0 {
        Version::new(0, mi + 1, 0)
    } else if minor.is_none() {
        // ^0 := >=0.0.0 <1.0.0
        Version::new(1, 0, 0)
    } else {
        Version::new(0, 0, pa + 1)
    };
    SemverRanges::between(start, end)
}

/// Detect a cycle among selected packages (LAR forbids dependency cycles).
fn detect_dependency_cycle(
    packages: &BTreeMap<(String, String), LockedPackage>,
) -> Option<(String, String)> {
    let by_id: BTreeMap<&str, &LockedPackage> =
        packages.values().map(|p| (p.id.as_str(), p)).collect();

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();

    fn dfs(
        id: &str,
        by_id: &BTreeMap<&str, &LockedPackage>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Option<(String, String)> {
        if visited.contains(id) {
            return None;
        }
        if !visiting.insert(id.to_string()) {
            let pkg = by_id.get(id)?;
            return Some((pkg.id.clone(), pkg.version.clone()));
        }
        if let Some(pkg) = by_id.get(id) {
            for dep_id in pkg.dependencies.keys() {
                if by_id.contains_key(dep_id.as_str()) {
                    if let Some(cycle) = dfs(dep_id, by_id, visiting, visited) {
                        return Some(cycle);
                    }
                }
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        None
    }

    for id in by_id.keys() {
        if let Some(cycle) = dfs(id, &by_id, &mut visiting, &mut visited) {
            return Some(cycle);
        }
    }
    None
}

fn materialize(store: &Store, packages: &BTreeMap<(String, String), LockedPackage>) -> Result<()> {
    let mut warn_out = io::stderr();
    for pin in packages.values() {
        let Some(expected_hash) = pin.content_hash.clone() else {
            continue;
        };
        let stored = if let Some(existing) = store.get(&pin.id, &pin.version)? {
            lar_repo::emit_store_hit_warnings(
                store,
                &pin.id,
                &pin.version,
                Some(&existing.content_hash),
                &mut warn_out,
            )?;
            existing
        } else {
            lar_repo::fetch_into_store(store, &pin.id, &pin.version, &mut warn_out).map_err(
                |err| match err {
                    lar_repo::Error::PackageNotFound { id, version } => {
                        Error::Missing { id, version }
                    }
                    other => other.into(),
                },
            )?
        };
        if stored.content_hash != expected_hash {
            return Err(Error::HashMismatch {
                id: pin.id.clone(),
                version: pin.version.clone(),
                locked: expected_hash,
                store: stored.content_hash,
            });
        }
        let manifest = load_manifest(&stored.path.join("package.toml"))?;
        if manifest.dependencies != pin.dependencies {
            return Err(Error::DependencyMismatch {
                id: pin.id.clone(),
                version: pin.version.clone(),
            });
        }
    }
    Ok(())
}
