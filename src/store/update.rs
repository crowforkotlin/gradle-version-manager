use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use super::types::UpdateCheck;
use super::version::compare_versions;

/// GitHub repository that publishes gvm release archives.
pub const DEFAULT_REPOSITORY: &str = "crowforkotlin/gradle-version-manager";

/// Version of the running binary.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Extract the release tag from a `github.com/<repo>/releases/tag/<tag>` URL.
pub(crate) fn tag_from_release_url(url: &str) -> Option<String> {
    let (_, tag) = url.split_once("/releases/tag/")?;
    let tag = tag.split(['?', '#']).next().unwrap_or(tag).trim();
    if tag.is_empty() {
        return None;
    }
    Some(tag.to_owned())
}

/// Build the check result for a resolved release tag.
///
/// Asset names are fixed by `.github/workflows/release.yml`, so the download
/// and checksum URLs follow from the tag and the platform target.
pub(crate) fn check_for_update(
    current: &str,
    repository: &str,
    tag: &str,
    target: &str,
) -> Result<UpdateCheck> {
    let latest = tag.strip_prefix("release-").unwrap_or(tag).to_owned();
    let archive_suffix = if target.starts_with("windows") {
        ".exe"
    } else {
        ".tar.gz"
    };
    let asset_name = format!("gvm-{latest}-{target}{archive_suffix}");
    let base = format!("https://github.com/{repository}/releases/download/{tag}");

    Ok(UpdateCheck {
        current: current.to_owned(),
        latest,
        tag: tag.to_owned(),
        download_url: format!("{base}/{asset_name}"),
        checksum_url: format!("{base}/{asset_name}.sha256"),
        asset_name,
        up_to_date: compare_versions(tag.strip_prefix("release-").unwrap_or(tag), current)
            != std::cmp::Ordering::Greater,
    })
}

/// Map the running platform onto the release target used in asset names.
pub fn release_target() -> Result<&'static str> {
    let android = cfg!(target_os = "android")
        || std::env::var_os("ANDROID_ROOT").is_some()
        || std::env::var_os("ANDROID_DATA").is_some();

    let target = match (android, std::env::consts::OS, std::env::consts::ARCH) {
        (true, _, "x86_64") => "android-x86_64",
        (true, _, "aarch64") => "android-aarch64",
        (false, "linux", "x86_64") => "linux-x86_64",
        (false, "linux", "aarch64") => "linux-aarch64",
        (false, "macos", "x86_64") => "darwin-x86_64",
        (false, "macos", "aarch64") => "darwin-aarch64",
        (false, "windows", "x86_64") => "windows-x86_64",
        (false, "windows", "aarch64") => "windows-aarch64",
        (true, _, arch) => bail!("no Android release archive is published for {arch}"),
        (false, os, arch) => bail!("no release archive is published for {os}-{arch}"),
    };

    Ok(target)
}

/// Path of the running executable, resolved through symlinks.
pub fn current_executable() -> Result<PathBuf> {
    let executable = std::env::current_exe().context("resolve the running gvm executable")?;
    executable
        .canonicalize()
        .context("canonicalize the running gvm executable")
}

/// Replace `install_path` with the binary contained in `archive_bytes`.
///
/// The archive is either a `.tar.gz` release archive containing
/// `<package>/gvm`, or a raw Windows `.exe`.
pub fn install_archive(archive_bytes: &[u8], asset_name: &str, install_path: &Path) -> Result<()> {
    let binary = if asset_name.ends_with(".exe") {
        archive_bytes.to_vec()
    } else {
        extract_binary_from_tar_gz(archive_bytes)?
    };
    if binary.is_empty() {
        bail!("refusing to install an empty gvm binary");
    }

    let directory = install_path
        .parent()
        .ok_or_else(|| anyhow!("install path {} has no parent", install_path.display()))?;
    fs::create_dir_all(directory)
        .with_context(|| format!("create directory {}", directory.display()))?;

    let file_name = install_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("gvm");
    let staging = directory.join(format!(".{file_name}.new-{}", std::process::id()));
    {
        let mut file = File::create(&staging)
            .with_context(|| format!("create staged binary {}", staging.display()))?;
        file.write_all(&binary)
            .with_context(|| format!("write staged binary {}", staging.display()))?;
        file.sync_all()
            .with_context(|| format!("flush staged binary {}", staging.display()))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("mark {} executable", staging.display()))?;
    }

    if cfg!(windows) && install_path.exists() {
        // Windows refuses to replace a running executable, but it allows renaming it.
        let retired = directory.join(format!(".{file_name}.old-{}", std::process::id()));
        let _ = fs::remove_file(&retired);
        fs::rename(install_path, &retired).with_context(|| {
            format!(
                "move the running binary {} out of the way",
                install_path.display()
            )
        })?;
        fs::rename(&staging, install_path)
            .with_context(|| format!("move the new binary to {}", install_path.display()))?;
        let _ = fs::remove_file(&retired);
        return Ok(());
    }

    fs::rename(&staging, install_path).with_context(|| {
        format!(
            "replace {} with the downloaded binary",
            install_path.display()
        )
    })?;
    Ok(())
}

/// Verify a downloaded archive against the published `.sha256` payload.
pub fn verify_download(bytes: &[u8], checksum_payload: &str, asset_name: &str) -> Result<()> {
    let expected = checksum_payload
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("empty checksum payload for {asset_name}"))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        bail!("checksum mismatch for {asset_name}: expected {expected}, got {actual}");
    }
    Ok(())
}

fn extract_binary_from_tar_gz(bytes: &[u8]) -> Result<Vec<u8>> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    let expected = if cfg!(windows) { "gvm.exe" } else { "gvm" };

    for entry in archive.entries().context("read release archive entries")? {
        let mut entry = entry.context("read release archive entry")?;
        let path = entry.path().context("read release archive entry path")?;
        let matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == expected);
        if !matches {
            continue;
        }

        let mut buffer = Vec::new();
        entry
            .read_to_end(&mut buffer)
            .context("read gvm binary from the release archive")?;
        return Ok(buffer);
    }

    Err(anyhow!("release archive does not contain {expected}"))
}

/// Read a checksum payload captured from the release asset.
pub(crate) fn checksum_from_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_from_release_url_extracts_the_tag() {
        assert_eq!(
            tag_from_release_url(
                "https://github.com/crowforkotlin/gradle-version-manager/releases/tag/release-1.0.2"
            )
            .as_deref(),
            Some("release-1.0.2")
        );
        assert_eq!(
            tag_from_release_url("https://github.com/o/r/releases/tag/release-2.0.0?x=1")
                .as_deref(),
            Some("release-2.0.0")
        );
        assert_eq!(
            tag_from_release_url("https://example.test/releases/latest"),
            None
        );
        assert_eq!(
            tag_from_release_url("https://github.com/o/r/releases/tag/").as_deref(),
            None
        );
    }

    #[test]
    fn check_for_update_builds_platform_asset_urls() {
        let check = check_for_update(
            "1.0.2",
            "crowforkotlin/gradle-version-manager",
            "release-1.0.3",
            "linux-x86_64",
        )
        .unwrap();

        assert_eq!(check.latest, "1.0.3");
        assert_eq!(check.asset_name, "gvm-1.0.3-linux-x86_64.tar.gz");
        assert_eq!(
            check.download_url,
            "https://github.com/crowforkotlin/gradle-version-manager/releases/download/release-1.0.3/gvm-1.0.3-linux-x86_64.tar.gz"
        );
        assert_eq!(
            check.checksum_url,
            "https://github.com/crowforkotlin/gradle-version-manager/releases/download/release-1.0.3/gvm-1.0.3-linux-x86_64.tar.gz.sha256"
        );
        assert!(!check.up_to_date);

        let windows = check_for_update(
            "1.0.2",
            "crowforkotlin/gradle-version-manager",
            "release-1.0.3",
            "windows-aarch64",
        )
        .unwrap();
        assert_eq!(windows.asset_name, "gvm-1.0.3-windows-aarch64.exe");
    }

    #[test]
    fn check_for_update_detects_an_up_to_date_binary() {
        let check = check_for_update(
            "1.0.2",
            "crowforkotlin/gradle-version-manager",
            "release-1.0.2",
            "linux-x86_64",
        )
        .unwrap();
        assert!(check.up_to_date);

        let older = check_for_update(
            "1.0.3",
            "crowforkotlin/gradle-version-manager",
            "release-1.0.2",
            "linux-x86_64",
        )
        .unwrap();
        assert!(older.up_to_date);
    }

    #[test]
    fn verify_download_accepts_and_rejects_checksums() {
        let bytes = b"release payload";
        let digest = format!("{:x}", Sha256::digest(bytes));

        verify_download(bytes, &format!("{digest}  gvm.tar.gz\n"), "gvm.tar.gz").unwrap();
        assert!(verify_download(bytes, &format!("{}  gvm.tar.gz\n", "0".repeat(64)), "a").is_err());
        assert!(verify_download(bytes, "", "gvm.tar.gz").is_err());
    }

    #[test]
    fn install_archive_replaces_the_binary_from_a_tar_gz() {
        let temp = tempfile::tempdir().unwrap();
        let install_path = temp.path().join("gvm");
        fs::write(&install_path, b"old binary").unwrap();

        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(b"new binary".len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "gvm-1.0.3-linux-x86_64/gvm",
                    &b"new binary"[..],
                )
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }

        install_archive(
            &archive_bytes,
            "gvm-1.0.3-linux-x86_64.tar.gz",
            &install_path,
        )
        .unwrap();
        assert_eq!(fs::read(&install_path).unwrap(), b"new binary");
    }

    #[test]
    fn install_archive_writes_a_raw_windows_executable() {
        let temp = tempfile::tempdir().unwrap();
        let install_path = temp.path().join("gvm.exe");

        install_archive(b"MZ payload", "gvm-1.0.3-windows-x86_64.exe", &install_path).unwrap();
        assert_eq!(fs::read(&install_path).unwrap(), b"MZ payload");
    }

    #[test]
    fn install_archive_rejects_an_archive_without_a_binary() {
        let temp = tempfile::tempdir().unwrap();
        let install_path = temp.path().join("gvm");
        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(b"readme".len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    "gvm-1.0.3-linux-x86_64/README.md",
                    &b"readme"[..],
                )
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }

        assert!(
            install_archive(
                &archive_bytes,
                "gvm-1.0.3-linux-x86_64.tar.gz",
                &install_path
            )
            .is_err()
        );
        assert!(!install_path.exists());
    }

    #[test]
    fn release_target_names_a_supported_platform() {
        let target = release_target().expect("the test host is a supported release target");
        assert!(target.starts_with("linux") || target.starts_with("darwin"));
    }
}
