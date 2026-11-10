use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::fs::resolve_gradle_user_home;
use super::types::{CacheCleanOptions, CacheCleanSummary, CacheEntry, CacheSelector};
use super::version::normalize_version;

/// List removable cache areas under the Gradle user home.
pub fn cache_entries() -> Result<Vec<CacheEntry>> {
    cache_entries_in(&resolve_gradle_user_home())
}

/// List removable cache areas under an explicit Gradle user home.
pub fn cache_entries_in(gradle_home: &Path) -> Result<Vec<CacheEntry>> {
    let mut entries = Vec::new();

    let caches = gradle_home.join("caches");
    if caches.is_dir() {
        for entry in fs::read_dir(&caches)
            .with_context(|| format!("read cache directory {}", caches.display()))?
        {
            let entry = entry.context("read cache entry")?;
            let path = entry.path();
            if !entry.file_type().context("read cache entry type")?.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(selector) = classify_cache_dir(&name) else {
                continue;
            };
            entries.push(measure_entry(selector, name, path));
        }
    }

    for (selector, path, label) in [
        (
            CacheSelector::DaemonLogs,
            gradle_home.join("daemon"),
            String::from("daemon logs"),
        ),
        (
            CacheSelector::WrapperDists,
            gradle_home.join("wrapper").join("dists"),
            String::from("wrapper dists"),
        ),
    ] {
        if path.is_dir() {
            let entry = measure_entry(selector, label, path);
            if entry.files > 0 {
                entries.push(entry);
            }
        }
    }

    // Empty directories hold nothing to reclaim; listing them only adds noise.
    // Version entries stay so `--version <version>` can still target them.
    entries.retain(|entry| entry.files > 0 || entry.selector == CacheSelector::Version);
    entries.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(entries)
}

/// Measure the cache areas selected by `options` without deleting anything.
pub fn plan_cache_clean(options: &CacheCleanOptions) -> Result<CacheCleanSummary> {
    plan_cache_clean_in(&resolve_gradle_user_home(), options)
}

/// Measure the cache areas selected by `options` under an explicit Gradle user home.
pub fn plan_cache_clean_in(
    gradle_home: &Path,
    options: &CacheCleanOptions,
) -> Result<CacheCleanSummary> {
    let selected = select_entries(gradle_home, options)?;
    Ok(CacheCleanSummary {
        freed_bytes: selected.iter().map(|entry| entry.bytes).sum(),
        removed: selected,
        dry_run: options.dry_run,
    })
}

/// Delete the cache areas recorded in a plan produced by [`plan_cache_clean`].
pub fn apply_cache_clean(plan: &CacheCleanSummary) -> Result<CacheCleanSummary> {
    let mut removed = Vec::with_capacity(plan.removed.len());
    let mut freed_bytes = 0;

    for entry in &plan.removed {
        remove_entry(entry)?;
        freed_bytes += entry.bytes;
        removed.push(entry.clone());
    }

    Ok(CacheCleanSummary {
        removed,
        freed_bytes,
        dry_run: false,
    })
}

fn select_entries(gradle_home: &Path, options: &CacheCleanOptions) -> Result<Vec<CacheEntry>> {
    if !options.all && options.selectors.is_empty() && options.versions.is_empty() {
        bail!("select a cache area with --selector, pass --version <version>, or pass --all");
    }

    let requested_versions = options
        .versions
        .iter()
        .map(|version| normalize_version(version))
        .collect::<Result<Vec<_>>>()?;
    let mut matched_versions = vec![false; requested_versions.len()];

    let entries = cache_entries_in(gradle_home)?;
    if options.all {
        return Ok(entries);
    }

    let mut selected = Vec::new();
    for entry in entries {
        let selector_match = options.selectors.contains(&entry.selector);
        let mut version_match = false;
        if entry.selector == CacheSelector::Version {
            for (index, version) in requested_versions.iter().enumerate() {
                if &entry.label == version {
                    version_match = true;
                    matched_versions[index] = true;
                }
            }
        }

        if selector_match || version_match {
            selected.push(entry);
        }
    }

    for (index, matched) in matched_versions.iter().enumerate() {
        if !matched {
            bail!(
                "no cache directory found for Gradle {}",
                requested_versions[index]
            );
        }
    }

    Ok(selected)
}

fn classify_cache_dir(name: &str) -> Option<CacheSelector> {
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        return Some(CacheSelector::Version);
    }
    if name.starts_with("modules-") {
        return Some(CacheSelector::Modules);
    }
    if name.starts_with("jars-") {
        return Some(CacheSelector::Jars);
    }
    if name.starts_with("transforms-") {
        return Some(CacheSelector::Transforms);
    }
    if name.starts_with("build-cache-") {
        return Some(CacheSelector::BuildCache);
    }
    if name.starts_with("journal-") {
        return Some(CacheSelector::Journal);
    }
    if name == "kotlin-dsl" {
        return Some(CacheSelector::KotlinDsl);
    }
    None
}

fn measure_entry(selector: CacheSelector, label: String, path: PathBuf) -> CacheEntry {
    let only_logs = selector == CacheSelector::DaemonLogs;
    let (bytes, files) = measure_tree(&path, only_logs);
    CacheEntry {
        label,
        selector,
        path,
        bytes,
        files,
    }
}

fn measure_tree(path: &Path, only_logs: bool) -> (u64, u64) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return (0, 0),
    };
    if metadata.file_type().is_symlink() {
        return (0, 0);
    }
    if metadata.is_file() {
        if only_logs
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".log"))
        {
            return (0, 0);
        }
        return (metadata.len(), 1);
    }
    if !metadata.is_dir() {
        return (0, 0);
    }

    let mut bytes = 0;
    let mut files = 0;
    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let (entry_bytes, entry_files) = measure_tree(&entry.path(), only_logs);
            bytes += entry_bytes;
            files += entry_files;
        }
    }
    (bytes, files)
}

fn remove_entry(entry: &CacheEntry) -> Result<()> {
    if entry.selector == CacheSelector::DaemonLogs {
        remove_log_files(&entry.path)?;
        return Ok(());
    }

    let metadata = fs::symlink_metadata(&entry.path)
        .with_context(|| format!("inspect cache path {}", entry.path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(&entry.path)
            .with_context(|| format!("remove cache directory {}", entry.path.display()))?;
    } else {
        fs::remove_file(&entry.path)
            .with_context(|| format!("remove cache file {}", entry.path.display()))?;
    }
    Ok(())
}

fn remove_log_files(directory: &Path) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", directory.display()));
        }
    };

    for entry in entries {
        let entry = entry.context("read daemon directory entry")?;
        let path = entry.path();
        let file_type = entry.file_type().context("read daemon entry type")?;
        if file_type.is_dir() {
            remove_log_files(&path)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".log"))
        {
            fs::remove_file(&path)
                .with_context(|| format!("remove daemon log {}", path.display()))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{apply_cache_clean, cache_entries_in, plan_cache_clean_in};
    use crate::store::types::{CacheCleanOptions, CacheSelector};

    fn gradle_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for (path, payload) in [
            ("caches/8.13/file.bin", "version cache"),
            ("caches/9.5.1/file.bin", "other version cache"),
            ("caches/modules-2/files-2.1/lib.jar", "dependency"),
            ("caches/jars-9/instrumented.jar", "jar"),
            ("caches/transforms-4/transformed.bin", "transform"),
            ("caches/build-cache-1/entry.bin", "build cache"),
            ("caches/journal-1/journal.bin", "journal"),
            ("caches/kotlin-dsl/accessors.bin", "kotlin dsl"),
            ("caches/user-id.txt", "unrelated"),
            ("daemon/8.13/daemon-1.out.log", "daemon log"),
            ("daemon/8.13/registry.bin", "registry"),
            (
                "wrapper/dists/gradle-8.13-bin/hash/gradle-8.13-bin.zip",
                "distribution",
            ),
        ] {
            let file = home.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, payload).unwrap();
        }
        home
    }

    /// Plan and, unless `dry_run` is set, apply a cache deletion.
    fn clean(
        home: &std::path::Path,
        selection: &CacheCleanOptions,
    ) -> anyhow::Result<super::CacheCleanSummary> {
        let plan = plan_cache_clean_in(home, selection)?;
        if selection.dry_run {
            return Ok(plan);
        }
        apply_cache_clean(&plan)
    }

    fn options(selectors: &[CacheSelector]) -> CacheCleanOptions {
        CacheCleanOptions {
            selectors: selectors.to_vec(),
            versions: Vec::new(),
            all: false,
            dry_run: false,
        }
    }

    #[test]
    fn cache_entries_classify_supported_directories() {
        let home = gradle_home();
        let entries = cache_entries_in(home.path()).unwrap();
        let areas = entries
            .iter()
            .map(|entry| (entry.selector, entry.label.as_str()))
            .collect::<Vec<_>>();

        assert!(areas.contains(&(CacheSelector::Version, "8.13")));
        assert!(areas.contains(&(CacheSelector::Version, "9.5.1")));
        assert!(areas.contains(&(CacheSelector::Modules, "modules-2")));
        assert!(areas.contains(&(CacheSelector::Jars, "jars-9")));
        assert!(areas.contains(&(CacheSelector::Transforms, "transforms-4")));
        assert!(areas.contains(&(CacheSelector::BuildCache, "build-cache-1")));
        assert!(areas.contains(&(CacheSelector::Journal, "journal-1")));
        assert!(areas.contains(&(CacheSelector::KotlinDsl, "kotlin-dsl")));
        assert!(areas.contains(&(CacheSelector::DaemonLogs, "daemon logs")));
        assert!(areas.contains(&(CacheSelector::WrapperDists, "wrapper dists")));
        assert!(!areas.iter().any(|(_, label)| *label == "user-id.txt"));

        let version = entries
            .iter()
            .find(|entry| entry.label == "8.13")
            .expect("version entry");
        assert_eq!(version.bytes, "version cache".len() as u64);
        assert_eq!(version.files, 1);
    }

    #[test]
    fn clean_requires_an_explicit_selection() {
        let home = gradle_home();
        assert!(clean(home.path(), &options(&[])).is_err());
        assert!(home.path().join("caches/modules-2").is_dir());
    }

    #[test]
    fn clean_removes_only_selected_areas() {
        let home = gradle_home();
        let summary = clean(home.path(), &options(&[CacheSelector::Modules])).unwrap();

        assert_eq!(summary.removed.len(), 1);
        assert_eq!(summary.freed_bytes, "dependency".len() as u64);
        assert!(!home.path().join("caches/modules-2").exists());
        assert!(home.path().join("caches/8.13").is_dir());
        assert!(home.path().join("caches/build-cache-1").is_dir());
    }

    #[test]
    fn clean_by_version_removes_one_version_cache() {
        let home = gradle_home();
        let mut selection = options(&[]);
        selection.versions = vec![String::from("gradle-8.13-bin")];

        let summary = clean(home.path(), &selection).unwrap();
        assert_eq!(summary.removed.len(), 1);
        assert!(!home.path().join("caches/8.13").exists());
        assert!(home.path().join("caches/9.5.1").is_dir());
    }

    #[test]
    fn clean_by_unknown_version_fails_without_deleting() {
        let home = gradle_home();
        let mut selection = options(&[CacheSelector::Modules]);
        selection.versions = vec![String::from("7.6")];

        assert!(clean(home.path(), &selection).is_err());
        assert!(home.path().join("caches/modules-2").is_dir());
        assert!(home.path().join("caches/8.13").is_dir());
    }

    #[test]
    fn dry_run_reports_without_deleting() {
        let home = gradle_home();
        let mut selection = options(&[CacheSelector::Modules]);
        selection.dry_run = true;

        let summary = clean(home.path(), &selection).unwrap();
        assert!(summary.dry_run);
        assert_eq!(summary.removed.len(), 1);
        assert!(home.path().join("caches/modules-2").is_dir());
    }

    #[test]
    fn daemon_logs_cleanup_keeps_non_log_files() {
        let home = gradle_home();
        let summary = clean(home.path(), &options(&[CacheSelector::DaemonLogs])).unwrap();

        assert_eq!(summary.removed.len(), 1);
        assert!(!home.path().join("daemon/8.13/daemon-1.out.log").exists());
        assert!(home.path().join("daemon/8.13/registry.bin").is_file());
    }

    #[test]
    fn empty_cache_directories_are_not_listed_but_stay_addressable_by_version() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("caches/transforms-4")).unwrap();
        fs::create_dir_all(home.path().join("caches/8.7")).unwrap();

        let entries = cache_entries_in(home.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].selector, CacheSelector::Version);
        assert_eq!(entries[0].files, 0);

        let mut selection = options(&[]);
        selection.versions = vec![String::from("8.7")];
        let summary = clean(home.path(), &selection).unwrap();
        assert_eq!(summary.removed.len(), 1);
        assert!(!home.path().join("caches/8.7").exists());
        assert!(home.path().join("caches/transforms-4").is_dir());
    }

    #[test]
    fn all_selects_every_removable_area() {
        let home = gradle_home();
        let mut selection = options(&[]);
        selection.all = true;

        let summary = clean(home.path(), &selection).unwrap();
        assert!(summary.removed.len() >= 10);
        assert!(!home.path().join("caches/modules-2").exists());
        assert!(!home.path().join("caches/8.13").exists());
        assert!(home.path().join("caches/user-id.txt").is_file());
    }
}
