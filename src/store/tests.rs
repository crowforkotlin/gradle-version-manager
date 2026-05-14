use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::Store;
use super::fs::{LinkKind, clean_wrapper_cache, launcher_name, replace_link, unzip_archive};
use super::remote::{
    InstallRequest, RemoteRelease, distribution_checksum_url, distribution_url,
    parse_install_request, select_latest_stable_release,
};
use super::scan_wrapper_dists;
use super::types::{
    AddMode, AddStatus, CleanSummary, DetectionSource, ReleaseChannel, RemoveStatus,
};
use super::version::normalize_version;

fn test_store(home: &Path) -> Store {
    Store::new(Some(home.to_path_buf())).expect("create store")
}

fn install_fake_version(home: &Path, version: &str) {
    let launcher = home
        .join("versions")
        .join(version)
        .join("bin")
        .join(launcher_name());
    fs::create_dir_all(launcher.parent().expect("launcher parent")).expect("create bin dir");
    fs::write(&launcher, launcher_stub()).expect("write launcher");
}

fn external_gradle_home(root: &Path, version: &str) -> PathBuf {
    let home = root.join(format!("gradle-{version}"));
    let launcher = home.join("bin").join(launcher_name());
    fs::create_dir_all(launcher.parent().expect("launcher parent")).expect("create bin dir");
    fs::create_dir_all(home.join("lib")).expect("create lib dir");
    fs::write(&launcher, launcher_stub()).expect("write launcher");
    fs::write(home.join("lib").join("dummy.txt"), b"ok").expect("write dummy lib file");
    home
}

fn remote_release(version: &str) -> RemoteRelease {
    RemoteRelease {
        version: String::from(version),
        current: false,
        snapshot: false,
        nightly: false,
        release_nightly: false,
        active_rc: false,
        rc_for: String::new(),
        milestone_for: String::new(),
        broken: false,
        download_url: distribution_url(version),
        checksum_url: distribution_checksum_url(version),
    }
}

fn launcher_stub() -> &'static [u8] {
    if cfg!(windows) {
        b"@echo off\r\n"
    } else {
        b"#!/bin/sh\n"
    }
}

#[test]
fn parse_install_request_supports_aliases() {
    assert_eq!(
        parse_install_request("current").unwrap(),
        InstallRequest::Current
    );
    assert_eq!(
        parse_install_request("latest").unwrap(),
        InstallRequest::Current
    );
    assert_eq!(
        parse_install_request("latest-8").unwrap(),
        InstallRequest::LatestMajor(8)
    );
    assert_eq!(
        parse_install_request(" gradle-8.13-bin ").unwrap(),
        InstallRequest::Exact(String::from("8.13"))
    );
    assert!(parse_install_request("lts").is_err());
}

#[test]
fn normalize_version_accepts_common_inputs() {
    assert_eq!(normalize_version("8.9").unwrap(), "8.9");
    assert_eq!(normalize_version(" gradle-8.9-bin ").unwrap(), "8.9");
    assert_eq!(normalize_version("gradle-8.13-all").unwrap(), "8.13");
    assert_eq!(
        normalize_version("gradle-9.5.0-milestone-7").unwrap(),
        "9.5.0-milestone-7"
    );
}

#[test]
fn normalize_version_rejects_path_separators() {
    assert!(normalize_version("../8.9").is_err());
    assert!(normalize_version(r"..\8.9").is_err());
}

#[test]
fn list_versions_marks_current_and_sorts_naturally() {
    let home = tempfile::tempdir().unwrap();
    let store = test_store(home.path());
    install_fake_version(home.path(), "8.9");
    install_fake_version(home.path(), "8.13");
    install_fake_version(home.path(), "9.5.0-milestone-7");
    install_fake_version(home.path(), "9.5.0");

    store.use_version("9.5.0").unwrap();
    let versions = store.list_versions().unwrap();

    assert_eq!(
        versions
            .iter()
            .map(|version| version.name.as_str())
            .collect::<Vec<_>>(),
        vec!["8.9", "8.13", "9.5.0", "9.5.0-milestone-7"]
    );
    assert!(versions[2].is_current);
}

#[test]
fn select_latest_stable_release_ignores_prereleases() {
    let mut releases = vec![
        remote_release("8.14.5"),
        RemoteRelease {
            milestone_for: String::from("9.6.0"),
            version: String::from("9.6.0-milestone-1"),
            ..remote_release("9.6.0-milestone-1")
        },
        remote_release("9.5.1"),
    ];
    releases[2].current = true;

    let selected = select_latest_stable_release(releases.iter()).unwrap();
    assert_eq!(selected.version, "9.5.1");
    assert_eq!(selected.channel(), ReleaseChannel::Stable);
}

#[test]
fn use_version_creates_current_and_launcher_links() {
    let home = tempfile::tempdir().unwrap();
    let store = test_store(home.path());
    install_fake_version(home.path(), "8.9");

    let selected = store.use_version("8.9").unwrap();
    assert_eq!(selected, "8.9");
    assert_eq!(store.current_version().unwrap(), Some(String::from("8.9")));

    let launcher_target = fs::read_link(home.path().join("bin").join(launcher_name())).unwrap();
    assert_eq!(
        launcher_target,
        PathBuf::from("..")
            .join("current")
            .join("bin")
            .join(launcher_name())
    );
}

#[test]
fn add_copy_creates_managed_copy() {
    let home = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = external_gradle_home(external.path(), "8.13");
    let store = test_store(home.path());

    let status = store.add_version(&source, None, AddMode::Copy).unwrap();
    assert_eq!(status, AddStatus::Added(String::from("8.13")));

    let managed = home.path().join("versions").join("8.13");
    assert!(managed.join("bin").join(launcher_name()).is_file());
    assert!(managed.join("lib").join("dummy.txt").is_file());
    assert!(
        !fs::symlink_metadata(&managed)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn add_link_creates_managed_symlink() {
    let home = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = external_gradle_home(external.path(), "8.13");
    let store = test_store(home.path());

    let status = store.add_version(&source, None, AddMode::Link).unwrap();
    assert_eq!(status, AddStatus::Added(String::from("8.13")));

    let managed = home.path().join("versions").join("8.13");
    assert!(
        fs::symlink_metadata(&managed)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        managed.canonicalize().unwrap(),
        source.canonicalize().unwrap()
    );
}

#[test]
fn remove_current_requires_force() {
    let home = tempfile::tempdir().unwrap();
    let store = test_store(home.path());
    install_fake_version(home.path(), "8.13");
    store.use_version("8.13").unwrap();

    assert!(store.remove_version("8.13", false).is_err());
    assert_eq!(
        store.remove_version("8.13", true).unwrap(),
        RemoveStatus::Removed(String::from("8.13"))
    );
    assert!(!home.path().join("versions").join("8.13").exists());
    assert!(store.current_version().unwrap().is_none());
}

#[test]
fn remove_link_only_removes_managed_symlink() {
    let home = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = external_gradle_home(external.path(), "8.13");
    let store = test_store(home.path());
    store.add_version(&source, None, AddMode::Link).unwrap();

    assert_eq!(
        store.remove_version("8.13", false).unwrap(),
        RemoveStatus::Removed(String::from("8.13"))
    );
    assert!(source.exists());
    assert!(!home.path().join("versions").join("8.13").exists());
}

#[test]
fn scan_wrapper_dists_finds_gradle_homes() {
    let home = tempfile::tempdir().unwrap();
    let store = test_store(home.path());
    let managed_homes = store.managed_homes().unwrap();
    let mut detected = BTreeMap::new();
    let root = home
        .path()
        .join(".gradle")
        .join("wrapper")
        .join("dists")
        .join("gradle-8.13-all")
        .join("hash");
    let gradle_home = external_gradle_home(&root, "8.13");

    scan_wrapper_dists(
        &home.path().join(".gradle").join("wrapper").join("dists"),
        &managed_homes,
        &mut detected,
    )
    .unwrap();

    let detected = detected.values().collect::<Vec<_>>();
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].source, DetectionSource::WrapperDists);
    assert_eq!(detected[0].version, "8.13");
    assert_eq!(detected[0].home, gradle_home.canonicalize().unwrap());
}

#[test]
fn clean_removes_tmp_broken_links_and_wrapper_partials() {
    let home = tempfile::tempdir().unwrap();
    let store = test_store(home.path());
    fs::create_dir_all(home.path().join("tmp")).unwrap();
    fs::write(home.path().join("tmp").join("partial.zip"), b"tmp").unwrap();

    let missing = home.path().join("missing-gradle");
    fs::create_dir_all(home.path().join("versions")).unwrap();
    replace_link(
        &missing,
        &home.path().join("versions").join("broken"),
        LinkKind::Directory,
    )
    .unwrap();
    fs::create_dir_all(home.path().join("bin")).unwrap();
    replace_link(
        Path::new("../current/bin").join(launcher_name()).as_path(),
        &home.path().join("bin").join(launcher_name()),
        LinkKind::File,
    )
    .unwrap();

    let wrapper_root = home
        .path()
        .join(".gradle")
        .join("wrapper")
        .join("dists")
        .join("gradle-8.13-bin")
        .join("hash");
    fs::create_dir_all(&wrapper_root).unwrap();
    fs::write(wrapper_root.join("gradle-8.13-bin.zip.part"), b"partial").unwrap();
    fs::write(wrapper_root.join("gradle-8.13-bin.zip.lck"), b"lock").unwrap();

    let wrapper_entries =
        clean_wrapper_cache(&home.path().join(".gradle").join("wrapper").join("dists")).unwrap();
    assert_eq!(wrapper_entries, 4);

    let summary: CleanSummary = store.clean(false).unwrap();
    assert_eq!(summary.gvm_entries, 3);
    assert!(!home.path().join("tmp").join("partial.zip").exists());
    assert!(!home.path().join("versions").join("broken").exists());
    assert!(!home.path().join("bin").join(launcher_name()).exists());
}

#[test]
fn unzip_archive_rejects_escaping_entries() {
    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("bad.zip");

    {
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../escape.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"bad").unwrap();
        writer.finish().unwrap();
    }

    let extract_dir = temp.path().join("extract");
    fs::create_dir_all(&extract_dir).unwrap();
    assert!(unzip_archive(&archive_path, &extract_dir).is_err());
}
