use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::types::ReleaseChannel;
use super::version::normalize_version;

/// Install selector accepted by `gvm install`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InstallRequest {
    Exact(String),
    Current,
    LatestMajor(u64),
}

/// Resolved version metadata needed for a concrete install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedInstall {
    pub version: String,
    pub download_url: String,
    pub checksum_url: String,
}

/// Raw release metadata returned by `services.gradle.org/versions`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RemoteRelease {
    pub version: String,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub snapshot: bool,
    #[serde(default)]
    pub nightly: bool,
    #[serde(rename = "releaseNightly", default)]
    pub release_nightly: bool,
    #[serde(rename = "activeRc", default)]
    pub active_rc: bool,
    #[serde(rename = "rcFor", default)]
    pub rc_for: String,
    #[serde(rename = "milestoneFor", default)]
    pub milestone_for: String,
    #[serde(default)]
    pub broken: bool,
    #[serde(rename = "downloadUrl")]
    pub download_url: String,
    #[serde(rename = "checksumUrl")]
    pub checksum_url: String,
}

impl RemoteRelease {
    /// Map the raw release flags into a single display channel.
    pub(super) fn channel(&self) -> ReleaseChannel {
        if self.broken {
            ReleaseChannel::Broken
        } else if self.active_rc || !self.rc_for.is_empty() {
            ReleaseChannel::ReleaseCandidate
        } else if !self.milestone_for.is_empty() {
            ReleaseChannel::Milestone
        } else if self.release_nightly {
            ReleaseChannel::ReleaseNightly
        } else if self.nightly {
            ReleaseChannel::Nightly
        } else if self.snapshot {
            ReleaseChannel::Snapshot
        } else {
            ReleaseChannel::Stable
        }
    }

    /// Whether the release is a stable Gradle build.
    pub(super) fn is_stable(&self) -> bool {
        self.channel() == ReleaseChannel::Stable
    }
}

impl From<RemoteRelease> for ResolvedInstall {
    fn from(value: RemoteRelease) -> Self {
        Self {
            version: value.version,
            download_url: value.download_url,
            checksum_url: value.checksum_url,
        }
    }
}

/// Build the official Gradle distribution URL for a concrete version.
pub(super) fn distribution_url(version: &str) -> String {
    format!("https://services.gradle.org/distributions/gradle-{version}-bin.zip")
}

/// Build the official checksum URL for a concrete version.
pub(super) fn distribution_checksum_url(version: &str) -> String {
    format!("{}.sha256", distribution_url(version))
}

/// Parse install aliases such as `current`, `latest`, and `latest-8`.
pub(super) fn parse_install_request(requested: &str) -> Result<InstallRequest> {
    let requested = requested.trim();
    let lowered = requested.to_ascii_lowercase();

    if lowered == "lts" {
        bail!("Gradle does not publish an official LTS line; use current or latest-<major>");
    }
    if lowered == "current" || lowered == "latest" {
        return Ok(InstallRequest::Current);
    }
    if let Some(major) = lowered.strip_prefix("latest-") {
        let major = major
            .parse::<u64>()
            .with_context(|| format!("parse major version from alias {requested}"))?;
        return Ok(InstallRequest::LatestMajor(major));
    }

    Ok(InstallRequest::Exact(normalize_version(requested)?))
}

/// Pick the newest stable release from an iterator of remote releases.
pub(super) fn select_latest_stable_release<'a>(
    releases: impl Iterator<Item = &'a RemoteRelease>,
) -> Option<&'a RemoteRelease> {
    releases
        .filter(|release| release.is_stable())
        .max_by(|left, right| super::version::compare_versions(&left.version, &right.version))
}
