mod fs;
mod remote;
#[cfg(test)]
mod tests;
mod types;
mod version;

use std::collections::BTreeMap;
use std::env;
use std::fs as stdfs;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Deserialize;

use self::fs::{
    LinkKind, clean_wrapper_cache, clear_directory_contents, copy_tree, ensure_gradle_home,
    infer_version_from_home, is_broken_symlink, launcher_name, locate_gradle_home,
    remove_existing_symlink, remove_link, replace_link, resolve_gradle_home,
    resolve_gradle_user_home, resolve_home, unzip_archive,
};
use self::remote::{
    RemoteRelease, ResolvedInstall, distribution_checksum_url, distribution_url,
    parse_install_request, select_latest_stable_release,
};
use self::version::{compare_versions, normalize_version, sanitize_version, version_major};

pub use self::types::{
    AddMode, AddStatus, CleanSummary, DetectedVersion, DetectionSource, InstallStatus,
    ListRemoteOptions, ManagedVersion, ReleaseChannel, RemoteVersionInfo, RemoveStatus,
};

const VERSION_SERVICE_ROOT: &str = "https://services.gradle.org/versions";

/// Filesystem-backed store for managed Gradle installations.
#[derive(Debug)]
pub struct Store {
    home: PathBuf,
    client: Client,
}

impl Store {
    /// Create a store using `GVM_HOME` or `~/.gvm` when no explicit home is provided.
    pub fn new(home: Option<PathBuf>) -> Result<Self> {
        let home = match home {
            Some(home) => home,
            None => resolve_home()?,
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("build HTTP client")?;

        Ok(Self { home, client })
    }

    /// Install a Gradle version into `~/.gvm/versions`.
    pub fn install_version(&self, requested: &str) -> Result<InstallStatus> {
        let release = self.resolve_install_release(requested)?;
        self.ensure_layout()?;

        let install_dir = self.version_dir(&release.version);
        if install_dir.join("bin").join(launcher_name()).is_file() {
            return Ok(InstallStatus::AlreadyInstalled(release.version));
        }
        if install_dir.exists() {
            bail!(
                "managed directory {} already exists but is incomplete",
                install_dir.display()
            );
        }

        let staging = self.make_staging_dir(&release.version)?;
        let archive_path = staging
            .path()
            .join(format!("gradle-{}-bin.zip", release.version));

        self.download_to_file(&release.download_url, &archive_path)?;
        self.verify_checksum(&release.checksum_url, &archive_path)?;

        let extract_dir = staging.path().join("extract");
        stdfs::create_dir_all(&extract_dir).context("create extraction directory")?;
        unzip_archive(&archive_path, &extract_dir)?;

        let gradle_home = locate_gradle_home(&extract_dir)?;
        stdfs::rename(&gradle_home, &install_dir).with_context(|| {
            format!(
                "move extracted Gradle home into managed directory {}",
                install_dir.display()
            )
        })?;

        self.ensure_launcher_link()?;
        Ok(InstallStatus::Installed(release.version))
    }

    /// List versions available from the official Gradle version service.
    pub fn list_remote_versions(
        &self,
        options: ListRemoteOptions,
    ) -> Result<Vec<RemoteVersionInfo>> {
        let mut releases = self.fetch_remote_releases(options.major)?;
        releases.sort_by(|left, right| compare_versions(&right.version, &left.version));

        Ok(releases
            .into_iter()
            .filter(|release| {
                options.include_prerelease || release.channel() == ReleaseChannel::Stable
            })
            .map(|release| {
                let channel = release.channel();
                RemoteVersionInfo {
                    version: release.version,
                    current: release.current,
                    channel,
                }
            })
            .collect())
    }

    /// Detect external Gradle homes available on the machine.
    pub fn detect_versions(&self) -> Result<Vec<DetectedVersion>> {
        let managed_homes = self.managed_homes()?;
        let mut detected = BTreeMap::new();

        if let Some(gradle_home) = env::var_os("GRADLE_HOME") {
            register_detected_home(
                PathBuf::from(gradle_home),
                DetectionSource::Environment,
                &managed_homes,
                &mut detected,
            )?;
        }

        if let Some(path) = env::var_os("PATH") {
            for directory in env::split_paths(&path) {
                let launcher = directory.join(launcher_name());
                if launcher.is_file() {
                    if let Ok(home) = resolve_gradle_home(&launcher) {
                        register_detected_home(
                            home,
                            DetectionSource::Path,
                            &managed_homes,
                            &mut detected,
                        )?;
                    }
                }
            }
        }

        scan_wrapper_dists(&self.wrapper_dists_root(), &managed_homes, &mut detected)?;
        self.scan_sdkman(&managed_homes, &mut detected)?;
        self.scan_common_prefixes(&managed_homes, &mut detected)?;

        let mut versions = detected.into_values().collect::<Vec<_>>();
        versions.sort_by(|left, right| {
            compare_versions(&left.version, &right.version)
                .then_with(|| left.source.cmp(&right.source))
                .then_with(|| left.home.cmp(&right.home))
        });
        Ok(versions)
    }

    /// Add an existing Gradle home into the managed store.
    ///
    /// By default this copies the source into `~/.gvm/versions/<version>` so the
    /// managed version remains valid even if Wrapper caches or external tools are cleaned.
    /// Use [`AddMode::Link`] to keep the external directory as the source of truth.
    pub fn add_version(
        &self,
        source: &Path,
        requested_name: Option<&str>,
        mode: AddMode,
    ) -> Result<AddStatus> {
        self.ensure_layout()?;

        let source_home = resolve_gradle_home(source)?;
        ensure_gradle_home(&source_home)?;

        let version = match requested_name {
            Some(name) => normalize_version(name)?,
            None => infer_version_from_home(&source_home)?,
        };

        let managed_dir = self.version_dir(&version);
        if managed_dir.exists() {
            let existing = managed_dir
                .canonicalize()
                .with_context(|| format!("canonicalize {}", managed_dir.display()))?;
            let source = source_home
                .canonicalize()
                .with_context(|| format!("canonicalize {}", source_home.display()))?;
            if existing == source {
                return Ok(AddStatus::AlreadyManaged(version));
            }

            bail!(
                "version {version} is already managed at {}",
                managed_dir.display()
            );
        }

        match mode {
            AddMode::Copy => {
                let staging = self.make_staging_dir(&version)?;
                let staged_home = staging.path().join(format!("gradle-{version}"));
                copy_tree(&source_home, &staged_home)?;
                stdfs::rename(&staged_home, &managed_dir).with_context(|| {
                    format!(
                        "move added Gradle home into managed directory {}",
                        managed_dir.display()
                    )
                })?;
            }
            AddMode::Link => {
                replace_link(&source_home, &managed_dir, LinkKind::Directory)?;
            }
        }

        self.ensure_launcher_link()?;
        Ok(AddStatus::Added(version))
    }

    /// List versions already managed by gvm.
    pub fn list_versions(&self) -> Result<Vec<ManagedVersion>> {
        self.ensure_layout()?;
        let current = self.current_version()?;
        let mut versions = Vec::new();

        for entry in stdfs::read_dir(self.versions_dir()).context("read versions directory")? {
            let entry = entry.context("read version entry")?;
            let path = entry.path();
            if !path.join("bin").join(launcher_name()).is_file() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            versions.push(ManagedVersion {
                is_current: current.as_deref() == Some(name.as_str()),
                name,
            });
        }

        versions.sort_by(|left, right| compare_versions(&left.name, &right.name));
        Ok(versions)
    }

    /// Remove a managed version from gvm.
    ///
    /// For copied or installed versions this deletes the managed directory.
    /// For linked versions it removes only the managed symlink, not the external source.
    pub fn remove_version(&self, requested: &str, force_current: bool) -> Result<RemoveStatus> {
        let version = normalize_version(requested)?;
        let managed_dir = self.version_dir(&version);
        let metadata = match stdfs::symlink_metadata(&managed_dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(RemoveStatus::NotInstalled(version));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect managed path {}", managed_dir.display()));
            }
        };

        let current = self.current_version()?;
        if current.as_deref() == Some(version.as_str()) && !force_current {
            bail!(
                "version {version} is currently selected; switch to another version first or pass --force"
            );
        }

        if metadata.file_type().is_symlink() {
            remove_link(&managed_dir, LinkKind::Directory)
                .with_context(|| format!("remove managed symlink {}", managed_dir.display()))?;
        } else if metadata.is_dir() {
            stdfs::remove_dir_all(&managed_dir)
                .with_context(|| format!("remove managed directory {}", managed_dir.display()))?;
        } else {
            bail!("managed path {} is not a directory", managed_dir.display());
        }

        if current.as_deref() == Some(version.as_str()) {
            remove_existing_symlink(&self.current_link(), LinkKind::Directory)?;
        }
        self.ensure_launcher_link()?;

        Ok(RemoveStatus::Removed(version))
    }

    /// Clean transient gvm data and optionally Wrapper cache remnants.
    pub fn clean(&self, wrapper_cache: bool) -> Result<CleanSummary> {
        let mut summary = CleanSummary {
            gvm_entries: 0,
            wrapper_entries: 0,
        };

        summary.gvm_entries += clear_directory_contents(&self.tmp_dir())?;

        if self.versions_dir().is_dir() {
            for entry in stdfs::read_dir(self.versions_dir()).context("read versions directory")? {
                let entry = entry.context("read version entry")?;
                let path = entry.path();
                if stdfs::symlink_metadata(&path)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
                    && is_broken_symlink(&path)?
                {
                    remove_link(&path, LinkKind::Directory)
                        .with_context(|| format!("remove broken symlink {}", path.display()))?;
                    summary.gvm_entries += 1;
                }
            }
        }

        for (path, kind) in [
            (self.current_link(), LinkKind::Directory),
            (self.bin_dir().join(launcher_name()), LinkKind::File),
        ] {
            if is_broken_symlink(&path)? {
                remove_link(&path, kind)
                    .with_context(|| format!("remove broken symlink {}", path.display()))?;
                summary.gvm_entries += 1;
            }
        }

        if wrapper_cache {
            summary.wrapper_entries += clean_wrapper_cache(&self.wrapper_dists_root())?;
        }

        Ok(summary)
    }

    /// Select which managed version `~/.gvm/bin/gradle` should resolve to.
    pub fn use_version(&self, requested: &str) -> Result<String> {
        let version = normalize_version(requested)?;
        self.ensure_layout()?;

        let version_dir = self.version_dir(&version);
        let launcher = version_dir.join("bin").join(launcher_name());
        if !launcher.is_file() {
            bail!("version {version} is not installed");
        }

        replace_link(&version_dir, &self.current_link(), LinkKind::Directory)?;
        self.ensure_launcher_link()?;
        Ok(version)
    }

    /// Return the currently selected managed version, if any.
    pub fn current_version(&self) -> Result<Option<String>> {
        let current_link = self.current_link();
        let target = match stdfs::read_link(&current_link) {
            Ok(target) => target,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("read current symlink {}", current_link.display()));
            }
        };

        let resolved = if target.is_absolute() {
            target
        } else {
            self.home.join(target)
        };

        Ok(resolved
            .file_name()
            .map(|component| component.to_string_lossy().into_owned()))
    }

    fn ensure_layout(&self) -> Result<()> {
        for dir in [
            &self.home,
            &self.versions_dir(),
            &self.bin_dir(),
            &self.tmp_dir(),
        ] {
            stdfs::create_dir_all(dir)
                .with_context(|| format!("create directory {}", dir.display()))?;
        }

        self.ensure_launcher_link()
    }

    fn ensure_launcher_link(&self) -> Result<()> {
        let relative_target = PathBuf::from("..")
            .join("current")
            .join("bin")
            .join(launcher_name());
        replace_link(
            &relative_target,
            &self.bin_dir().join(launcher_name()),
            LinkKind::File,
        )
    }

    fn make_staging_dir(&self, version: &str) -> Result<tempfile::TempDir> {
        tempfile::Builder::new()
            .prefix(&format!("install-{}-", sanitize_version(version)))
            .tempdir_in(self.tmp_dir())
            .context("create temporary install directory")
    }

    fn download_to_file(&self, url: &str, destination: &Path) -> Result<()> {
        let mut response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("download {url}"))?;

        if response.status() != StatusCode::OK {
            bail!("download {url}: unexpected status {}", response.status());
        }

        let mut file = File::create(destination)
            .with_context(|| format!("create archive {}", destination.display()))?;
        response
            .copy_to(&mut file)
            .with_context(|| format!("write archive {}", destination.display()))?;
        file.flush()
            .with_context(|| format!("flush archive {}", destination.display()))?;
        Ok(())
    }

    fn verify_checksum(&self, checksum_url: &str, archive_path: &Path) -> Result<()> {
        use sha2::{Digest, Sha256};

        let expected = self.fetch_checksum(checksum_url)?;
        let mut archive = File::open(archive_path)
            .with_context(|| format!("open archive {}", archive_path.display()))?;
        let mut hasher = Sha256::new();
        io::copy(&mut archive, &mut hasher)
            .with_context(|| format!("hash archive {}", archive_path.display()))?;
        let actual = format!("{:x}", hasher.finalize());

        if actual != expected {
            bail!(
                "checksum mismatch for {}: expected {expected}, got {actual}",
                archive_path.display()
            );
        }

        Ok(())
    }

    fn fetch_checksum(&self, checksum_url: &str) -> Result<String> {
        let response = self
            .client
            .get(checksum_url)
            .send()
            .with_context(|| format!("download {checksum_url}"))?;

        if response.status() != StatusCode::OK {
            bail!(
                "download {checksum_url}: unexpected status {}",
                response.status()
            );
        }

        let mut reader = BufReader::new(response);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .with_context(|| format!("read checksum payload from {checksum_url}"))?;

        let checksum = line
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow!("empty checksum response from {checksum_url}"))?;

        Ok(checksum.to_ascii_lowercase())
    }

    fn resolve_install_release(&self, requested: &str) -> Result<ResolvedInstall> {
        match parse_install_request(requested)? {
            remote::InstallRequest::Current => Ok(self.fetch_current_release()?.into()),
            remote::InstallRequest::LatestMajor(major) => {
                let releases = self.fetch_remote_releases(Some(major))?;
                let release = select_latest_stable_release(releases.iter()).ok_or_else(|| {
                    anyhow!("no stable Gradle release found for major line {major}")
                })?;
                Ok(release.clone().into())
            }
            remote::InstallRequest::Exact(version) => {
                if let Some(release) = self.find_remote_release(&version)? {
                    Ok(release.into())
                } else {
                    Ok(ResolvedInstall {
                        checksum_url: distribution_checksum_url(&version),
                        download_url: distribution_url(&version),
                        version,
                    })
                }
            }
        }
    }

    fn fetch_current_release(&self) -> Result<RemoteRelease> {
        self.fetch_remote_json(&format!("{VERSION_SERVICE_ROOT}/current"))
    }

    fn fetch_remote_releases(&self, major: Option<u64>) -> Result<Vec<RemoteRelease>> {
        let mut releases: Vec<RemoteRelease> =
            self.fetch_remote_json(&format!("{VERSION_SERVICE_ROOT}/all"))?;
        if let Some(major) = major {
            releases.retain(|release| version_major(&release.version) == Some(major));
        }
        Ok(releases)
    }

    fn fetch_remote_json<T>(&self, url: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("download {url}"))?;
        if response.status() != StatusCode::OK {
            bail!("download {url}: unexpected status {}", response.status());
        }

        response
            .json::<T>()
            .with_context(|| format!("parse JSON from {url}"))
    }

    fn find_remote_release(&self, version: &str) -> Result<Option<RemoteRelease>> {
        let releases = self.fetch_remote_releases(None)?;
        Ok(releases
            .into_iter()
            .find(|release| release.version == version))
    }

    fn managed_homes(&self) -> Result<BTreeMap<PathBuf, String>> {
        let mut homes = BTreeMap::new();
        if !self.versions_dir().is_dir() {
            return Ok(homes);
        }

        for entry in stdfs::read_dir(self.versions_dir()).context("read versions directory")? {
            let entry = entry.context("read version entry")?;
            let path = entry.path();
            if !path.join("bin").join(launcher_name()).is_file() {
                continue;
            }

            let canonical = path
                .canonicalize()
                .with_context(|| format!("canonicalize {}", path.display()))?;
            homes.insert(canonical, entry.file_name().to_string_lossy().into_owned());
        }

        Ok(homes)
    }

    fn scan_sdkman(
        &self,
        managed_homes: &BTreeMap<PathBuf, String>,
        detected: &mut BTreeMap<PathBuf, DetectedVersion>,
    ) -> Result<()> {
        let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
            return Ok(());
        };
        let sdkman_root = home.join(".sdkman").join("candidates").join("gradle");
        if !sdkman_root.is_dir() {
            return Ok(());
        }

        for entry in stdfs::read_dir(&sdkman_root)
            .with_context(|| format!("read {}", sdkman_root.display()))?
        {
            let entry = entry.context("read SDKMAN Gradle entry")?;
            let candidate = entry.path();
            if candidate.join("bin").join(launcher_name()).is_file() {
                register_detected_home(
                    candidate,
                    DetectionSource::Sdkman,
                    managed_homes,
                    detected,
                )?;
            }
        }

        Ok(())
    }

    fn scan_common_prefixes(
        &self,
        managed_homes: &BTreeMap<PathBuf, String>,
        detected: &mut BTreeMap<PathBuf, DetectedVersion>,
    ) -> Result<()> {
        for root in [Path::new("/opt"), Path::new("/usr/local")] {
            if !root.is_dir() {
                continue;
            }

            for entry in
                stdfs::read_dir(root).with_context(|| format!("read {}", root.display()))?
            {
                let entry = entry.context("read common-prefix entry")?;
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                let candidate = entry.path();

                if !name.contains("gradle") {
                    continue;
                }

                if candidate.join("bin").join(launcher_name()).is_file() {
                    register_detected_home(
                        candidate,
                        DetectionSource::CommonPrefix,
                        managed_homes,
                        detected,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn versions_dir(&self) -> PathBuf {
        self.home.join("versions")
    }

    fn bin_dir(&self) -> PathBuf {
        self.home.join("bin")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.home.join("tmp")
    }

    fn current_link(&self) -> PathBuf {
        self.home.join("current")
    }

    fn version_dir(&self, version: &str) -> PathBuf {
        self.versions_dir().join(version)
    }

    fn wrapper_dists_root(&self) -> PathBuf {
        resolve_gradle_user_home().join("wrapper").join("dists")
    }
}

fn scan_wrapper_dists(
    root: &Path,
    managed_homes: &BTreeMap<PathBuf, String>,
    detected: &mut BTreeMap<PathBuf, DetectedVersion>,
) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }

    for distribution in stdfs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let distribution = distribution.context("read wrapper distribution entry")?;
        if !distribution
            .file_type()
            .context("read wrapper distribution type")?
            .is_dir()
        {
            continue;
        }

        for hash_dir in stdfs::read_dir(distribution.path())
            .with_context(|| format!("read {}", distribution.path().display()))?
        {
            let hash_dir = hash_dir.context("read wrapper hash entry")?;
            if !hash_dir
                .file_type()
                .context("read wrapper hash type")?
                .is_dir()
            {
                continue;
            }

            for maybe_home in stdfs::read_dir(hash_dir.path())
                .with_context(|| format!("read {}", hash_dir.path().display()))?
            {
                let maybe_home = maybe_home.context("read wrapper extracted entry")?;
                let candidate = maybe_home.path();
                if candidate.join("bin").join(launcher_name()).is_file() {
                    register_detected_home(
                        candidate,
                        DetectionSource::WrapperDists,
                        managed_homes,
                        detected,
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn register_detected_home(
    home: PathBuf,
    source: DetectionSource,
    managed_homes: &BTreeMap<PathBuf, String>,
    detected: &mut BTreeMap<PathBuf, DetectedVersion>,
) -> Result<()> {
    let home = resolve_gradle_home(&home)?;
    ensure_gradle_home(&home)?;

    let version = infer_version_from_home(&home)?;
    let managed_as = managed_homes.get(&home).cloned();
    detected.entry(home.clone()).or_insert(DetectedVersion {
        version,
        home,
        source,
        managed_as,
    });
    Ok(())
}
