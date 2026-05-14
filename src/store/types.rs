use std::path::PathBuf;

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
