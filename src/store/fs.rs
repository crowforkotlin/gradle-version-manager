use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use zip::ZipArchive;

use super::version::normalize_version;

#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink as symlink_path};
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Filesystem kind used when creating or removing links.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum LinkKind {
    File,
    Directory,
}

/// Resolve `GVM_HOME` or fall back to `~/.gvm`.
pub(super) fn resolve_home() -> Result<PathBuf> {
    if let Some(home) = env::var_os("GVM_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }

    let user_home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("unable to resolve HOME for gvm storage"))?;
    Ok(user_home.join(".gvm"))
}

/// Resolve `GRADLE_USER_HOME` or fall back to `~/.gradle`.
pub(super) fn resolve_gradle_user_home() -> PathBuf {
    env::var_os("GRADLE_USER_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".gradle")))
        .unwrap_or_else(|| PathBuf::from(".gradle"))
}

/// Return the platform-specific Gradle launcher filename.
pub(super) fn launcher_name() -> &'static str {
    if cfg!(windows) {
        "gradle.bat"
    } else {
        "gradle"
    }
}

/// Resolve either a Gradle home or a launcher path to the canonical Gradle home.
pub(super) fn resolve_gradle_home(path: &Path) -> Result<PathBuf> {
    if path.is_file() && path.file_name().is_some_and(|name| name == launcher_name()) {
        let bin_dir = path
            .parent()
            .ok_or_else(|| anyhow!("launcher path {} has no parent", path.display()))?;
        let home = bin_dir
            .parent()
            .ok_or_else(|| anyhow!("launcher path {} has no Gradle home parent", path.display()))?;
        return home
            .canonicalize()
            .with_context(|| format!("canonicalize {}", home.display()));
    }

    if path.join("bin").join(launcher_name()).is_file() {
        return path
            .canonicalize()
            .with_context(|| format!("canonicalize {}", path.display()));
    }

    bail!(
        "{} is neither a Gradle home nor a Gradle launcher",
        path.display()
    )
}

/// Check that a directory looks like a Gradle home.
pub(super) fn ensure_gradle_home(home: &Path) -> Result<()> {
    let launcher = home.join("bin").join(launcher_name());
    if !launcher.is_file() {
        bail!("{} does not look like a Gradle home", home.display());
    }

    Ok(())
}

/// Infer the Gradle version from the directory name or from `gradle --version`.
pub(super) fn infer_version_from_home(home: &Path) -> Result<String> {
    if let Some(version) = version_from_name(home.file_name()) {
        return Ok(version);
    }

    let launcher = home.join("bin").join(launcher_name());
    let output = Command::new(&launcher)
        .arg("--version")
        .output()
        .with_context(|| format!("run {} --version", launcher.display()))?;
    if !output.status.success() {
        bail!(
            "{} --version exited with {}",
            launcher.display(),
            output.status
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(version) = line.strip_prefix("Gradle ") {
            return normalize_version(version.trim());
        }
    }

    bail!("unable to parse Gradle version from {}", launcher.display())
}

/// Find the extracted Gradle home inside an unpacked distribution.
pub(super) fn locate_gradle_home(extract_dir: &Path) -> Result<PathBuf> {
    let mut directories = Vec::new();
    for entry in fs::read_dir(extract_dir).context("read extracted archive")? {
        let entry = entry.context("read extracted entry")?;
        if entry
            .file_type()
            .context("read extracted entry type")?
            .is_dir()
        {
            directories.push(entry.path());
        }
    }

    if directories.len() != 1 {
        bail!(
            "expected one Gradle directory in archive, found {}",
            directories.len()
        );
    }

    let gradle_home = directories.pop().expect("directory count already checked");
    ensure_gradle_home(&gradle_home)?;
    Ok(gradle_home)
}

/// Extract a zip archive while preventing directory traversal.
pub(super) fn unzip_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let archive = File::open(archive_path)
        .with_context(|| format!("open archive {}", archive_path.display()))?;
    let mut zip = ZipArchive::new(archive)
        .with_context(|| format!("read zip archive {}", archive_path.display()))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .with_context(|| format!("read zip entry #{index}"))?;
        let enclosed = entry
            .enclosed_name()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("archive entry escapes extraction root: {}", entry.name()))?;
        let output_path = destination.join(enclosed);

        if entry.name().ends_with('/') {
            fs::create_dir_all(&output_path)
                .with_context(|| format!("create directory {}", output_path.display()))?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create directory {}", parent.display()))?;
        }

        let mut output = File::create(&output_path)
            .with_context(|| format!("create file {}", output_path.display()))?;
        io::copy(&mut entry, &mut output)
            .with_context(|| format!("extract file {}", output_path.display()))?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode))
                .with_context(|| format!("set unix permissions for {}", output_path.display()))?;
        }
    }

    Ok(())
}

/// Copy a Gradle home recursively into the managed store.
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("read metadata for {}", source.display()))?;
    if !metadata.is_dir() {
        bail!("{} is not a directory", source.display());
    }

    fs::create_dir_all(destination)
        .with_context(|| format!("create directory {}", destination.display()))?;
    fs::set_permissions(destination, metadata.permissions())
        .with_context(|| format!("set permissions for {}", destination.display()))?;

    for entry in fs::read_dir(source).with_context(|| format!("read {}", source.display()))? {
        let entry = entry.context("read source entry")?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let entry_metadata = fs::symlink_metadata(&source_path)
            .with_context(|| format!("read metadata for {}", source_path.display()))?;

        if entry_metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_path)
                .with_context(|| format!("read symlink {}", source_path.display()))?;
            let followed = fs::metadata(&source_path)
                .with_context(|| format!("follow symlink {}", source_path.display()))?;
            let kind = if followed.is_dir() {
                LinkKind::Directory
            } else {
                LinkKind::File
            };
            create_link(&target, &destination_path, kind).with_context(|| {
                format!(
                    "recreate symlink {} -> {}",
                    destination_path.display(),
                    target.display()
                )
            })?;
            continue;
        }

        if entry_metadata.is_dir() {
            copy_tree(&source_path, &destination_path)?;
            continue;
        }

        if entry_metadata.is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "copy file {} -> {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
            fs::set_permissions(&destination_path, entry_metadata.permissions())
                .with_context(|| format!("set permissions for {}", destination_path.display()))?;
            continue;
        }

        bail!("unsupported filesystem entry {}", source_path.display());
    }

    Ok(())
}

/// Remove every entry under a directory but keep the directory itself.
pub(super) fn clear_directory_contents(path: &Path) -> Result<usize> {
    if !path.is_dir() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry.context("read directory entry")?;
        remove_fs_entry(&entry.path())?;
        removed += 1;
    }
    Ok(removed)
}

/// Return whether a path is a symlink whose target no longer exists.
pub(super) fn is_broken_symlink(path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };
    if !metadata.file_type().is_symlink() {
        return Ok(false);
    }

    let target = fs::read_link(path).with_context(|| format!("read symlink {}", path.display()))?;
    let resolved = if target.is_absolute() {
        target
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).join(target)
    };

    Ok(!resolved.exists())
}

/// Remove a symlink only if it exists.
pub(super) fn remove_existing_symlink(path: &Path, kind: LinkKind) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_symlink() {
                bail!("{} exists and is not a symlink", path.display());
            }
            remove_link(path, kind).with_context(|| format!("remove symlink {}", path.display()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

/// Remove stale temporary files from Wrapper caches.
pub(super) fn clean_wrapper_cache(root: &Path) -> Result<usize> {
    if !root.is_dir() {
        return Ok(0);
    }

    let mut removed = 0;
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry.context("read wrapper cache entry")?;
        removed += clean_wrapper_cache_entry(&entry.path())?;
    }
    Ok(removed)
}

/// Replace a symlink atomically enough for local filesystem use.
pub(super) fn replace_link(target: &Path, link: &Path, kind: LinkKind) -> Result<()> {
    match fs::symlink_metadata(link) {
        Ok(metadata) => {
            if !metadata.file_type().is_symlink() {
                bail!("{} exists and is not a symlink", link.display());
            }

            let current_target =
                fs::read_link(link).with_context(|| format!("read symlink {}", link.display()))?;
            if current_target == target {
                return Ok(());
            }

            remove_link(link, kind)
                .with_context(|| format!("remove stale symlink {}", link.display()))?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect existing path {}", link.display()));
        }
    }

    create_link(target, link, kind)
        .with_context(|| format!("create symlink {} -> {}", link.display(), target.display()))
}

#[cfg(unix)]
fn create_link(target: &Path, link: &Path, _kind: LinkKind) -> io::Result<()> {
    symlink_path(target, link)
}

#[cfg(windows)]
fn create_link(target: &Path, link: &Path, kind: LinkKind) -> io::Result<()> {
    match kind {
        LinkKind::File => symlink_file(target, link),
        LinkKind::Directory => symlink_dir(target, link),
    }
}

#[cfg(unix)]
pub(super) fn remove_link(link: &Path, _kind: LinkKind) -> io::Result<()> {
    fs::remove_file(link)
}

#[cfg(windows)]
pub(super) fn remove_link(link: &Path, kind: LinkKind) -> io::Result<()> {
    match kind {
        LinkKind::File => fs::remove_file(link),
        LinkKind::Directory => fs::remove_dir(link),
    }
}

fn version_from_name(component: Option<&OsStr>) -> Option<String> {
    let text = component?.to_string_lossy();
    if !text.chars().any(|character| character.is_ascii_digit()) {
        return None;
    }

    normalize_version(&text).ok()
}

fn remove_fs_entry(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("read metadata for {}", path.display()))?;

    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("remove file {}", path.display()))?;
        return Ok(());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove directory {}", path.display()))?;
        return Ok(());
    }

    bail!("unsupported filesystem entry {}", path.display())
}

fn clean_wrapper_cache_entry(path: &Path) -> Result<usize> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("read metadata for {}", path.display()))?;

    if metadata.is_dir() {
        let mut removed = 0;
        for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
            let entry = entry.context("read wrapper child entry")?;
            removed += clean_wrapper_cache_entry(&entry.path())?;
        }

        if fs::read_dir(path)
            .with_context(|| format!("read {}", path.display()))?
            .next()
            .is_none()
        {
            fs::remove_dir(path)
                .with_context(|| format!("remove empty directory {}", path.display()))?;
            removed += 1;
        }
        return Ok(removed);
    }

    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    let removable =
        name.ends_with(".part") || name.ends_with(".lck") || name.ends_with(".sha256.part");
    if removable {
        fs::remove_file(path)
            .with_context(|| format!("remove wrapper cache file {}", path.display()))?;
        return Ok(1);
    }

    Ok(0)
}
