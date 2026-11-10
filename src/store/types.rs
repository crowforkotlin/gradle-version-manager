use std::path::PathBuf;

use clap::ValueEnum;

/// A Gradle version already managed by gvm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedVersion {
    pub name: String,
    pub is_current: bool,
}

/// Result of an install operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    Installed(String),
    AlreadyInstalled(String),
}

/// How `gvm add` should register an external Gradle home.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AddMode {
    Copy,
    Link,
}

/// Result of an add operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddStatus {
    Added(String),
    AlreadyManaged(String),
}

/// Result of a remove operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveStatus {
    Removed(String),
    NotInstalled(String),
}

/// Summary returned by `gvm clean`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CleanSummary {
    pub gvm_entries: usize,
    pub wrapper_entries: usize,
}

/// Options controlling remote version listing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ListRemoteOptions {
    pub major: Option<u64>,
    pub include_prerelease: bool,
}

/// A version returned from the official Gradle version service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVersionInfo {
    pub version: String,
    pub current: bool,
    pub channel: ReleaseChannel,
}

/// Release channel classification for remote Gradle versions.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    ReleaseCandidate,
    Milestone,
    ReleaseNightly,
    Nightly,
    Snapshot,
    Broken,
}

impl ReleaseChannel {
    /// Short human-readable label for display output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::ReleaseCandidate => "rc",
            Self::Milestone => "milestone",
            Self::ReleaseNightly => "release-nightly",
            Self::Nightly => "nightly",
            Self::Snapshot => "snapshot",
            Self::Broken => "broken",
        }
    }
}

/// A Gradle home detected outside `~/.gvm/versions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedVersion {
    pub version: String,
    pub home: PathBuf,
    pub source: DetectionSource,
    pub managed_as: Option<String>,
}

/// Origin of a detected external Gradle installation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetectionSource {
    Environment,
    Path,
    WrapperDists,
    Sdkman,
    CommonPrefix,
}

impl DetectionSource {
    /// Short source label used in CLI tables.
    pub fn label(self) -> &'static str {
        match self {
            Self::Environment => "env",
            Self::Path => "path",
            Self::WrapperDists => "wrapper",
            Self::Sdkman => "sdkman",
            Self::CommonPrefix => "system",
        }
    }
}

/// A removable area inside the Gradle user home.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CacheSelector {
    /// Version-specific cache directories such as `caches/8.13`
    Version,
    /// Dependency cache `caches/modules-2`
    Modules,
    /// Instrumented jar cache `caches/jars-*`
    Jars,
    /// Artifact transform cache `caches/transforms-*`
    Transforms,
    /// Local build cache `caches/build-cache-*`
    BuildCache,
    /// Cache journal `caches/journal-*`
    Journal,
    /// `caches/kotlin-dsl`
    KotlinDsl,
    /// Daemon logs under `daemon/`
    DaemonLogs,
    /// Downloaded Gradle distributions under `wrapper/dists`
    WrapperDists,
}

impl CacheSelector {
    /// Short label used in CLI output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Modules => "modules",
            Self::Jars => "jars",
            Self::Transforms => "transforms",
            Self::BuildCache => "build-cache",
            Self::Journal => "journal",
            Self::KotlinDsl => "kotlin-dsl",
            Self::DaemonLogs => "daemon-logs",
            Self::WrapperDists => "wrapper-dists",
        }
    }
}

/// One measured cache directory inside the Gradle user home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub label: String,
    pub selector: CacheSelector,
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
}

/// Cache areas the user asked to delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheCleanOptions {
    pub selectors: Vec<CacheSelector>,
    pub versions: Vec<String>,
    pub all: bool,
    pub dry_run: bool,
}

/// Result of a `gvm cache` deletion run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheCleanSummary {
    pub removed: Vec<CacheEntry>,
    pub freed_bytes: u64,
    pub dry_run: bool,
}

/// Kind of long-running Gradle process `gvm stop` can terminate.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum DaemonKind {
    /// `org.gradle.launcher.daemon.bootstrap.GradleDaemon`
    Gradle,
    /// Kotlin compile daemon started by the Kotlin Gradle plugin
    Kotlin,
}

impl DaemonKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gradle => "gradle",
            Self::Kotlin => "kotlin",
        }
    }
}

/// A running Gradle-related process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonProcess {
    pub pid: u32,
    pub kind: DaemonKind,
    pub command: String,
}

/// Result of one termination attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopOutcome {
    pub process: DaemonProcess,
    pub stopped: bool,
    pub detail: Option<String>,
}

/// Summary returned by `gvm stop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSummary {
    pub outcomes: Vec<StopOutcome>,
    pub dry_run: bool,
}

/// Gradle version evidence for one discovered project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectUsage {
    pub path: PathBuf,
    /// Version declared by `gradle/wrapper/gradle-wrapper.properties`.
    pub wrapper_version: Option<String>,
    /// Versions for which the project has a `.gradle/<version>` directory.
    pub observed_versions: Vec<String>,
}

/// Options controlling `gvm projects`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScanOptions {
    pub roots: Vec<PathBuf>,
    pub depth: usize,
}

/// A GitHub release candidate for `gvm update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub tag: String,
    pub asset_name: String,
    pub download_url: String,
    pub checksum_url: String,
    pub up_to_date: bool,
}
