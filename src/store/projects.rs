use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::types::{ProjectScanOptions, ProjectUsage};
use super::version::{compare_versions, normalize_version};

/// Directory names that are never descended into while looking for Gradle projects.
const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".gradle",
    ".idea",
    ".kotlin",
    ".fleet",
    "node_modules",
    "build",
    "target",
    "out",
    "dist",
    "vendor",
];

/// Find projects below `roots` and report the Gradle versions they use.
pub fn scan_projects(options: &ProjectScanOptions) -> Result<Vec<ProjectUsage>> {
    let depth = options.depth.max(1);
    let roots = if options.roots.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        options.roots.clone()
    };

    let mut projects = Vec::new();
    for root in roots {
        let canonical = root
            .canonicalize()
            .with_context(|| format!("resolve project root {}", root.display()))?;
        walk(&canonical, depth, &mut projects)?;
    }

    projects.sort_by(|left, right| left.path.cmp(&right.path));
    projects.dedup_by(|left, right| left.path == right.path);
    Ok(projects)
}

/// Read the versions a single project directory refers to.
pub fn inspect_project(path: &Path) -> Result<ProjectUsage> {
    let wrapper_version = read_wrapper_version(path)?;
    let observed_versions = read_observed_versions(path)?;
    Ok(ProjectUsage {
        path: path.to_path_buf(),
        wrapper_version,
        observed_versions,
    })
}

fn walk(directory: &Path, remaining_depth: usize, projects: &mut Vec<ProjectUsage>) -> Result<()> {
    let usage = inspect_project(directory)?;
    if usage.wrapper_version.is_some() || !usage.observed_versions.is_empty() {
        projects.push(usage);
    }

    if remaining_depth <= 1 {
        return Ok(());
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        if SKIPPED_DIRECTORIES.contains(&name.as_ref()) {
            continue;
        }

        walk(&entry.path(), remaining_depth - 1, projects)?;
    }

    Ok(())
}

fn read_wrapper_version(project: &Path) -> Result<Option<String>> {
    let properties = project
        .join("gradle")
        .join("wrapper")
        .join("gradle-wrapper.properties");
    let contents = match fs::read_to_string(&properties) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("read {}", properties.display()));
        }
    };

    Ok(contents
        .lines()
        .find_map(|line| parse_distribution_url(line).map(str::to_owned))
        .and_then(|url| distribution_version(&url)))
}

/// Extract the version from a `distributionUrl=` line.
fn parse_distribution_url(line: &str) -> Option<&str> {
    let line = line.trim().trim_start_matches('#').trim();
    let (key, value) = line.split_once('=')?;
    if !key.trim().eq_ignore_ascii_case("distributionUrl") {
        return None;
    }
    Some(value.trim())
}

/// Derive a Gradle version from a distribution file name.
///
/// Handles `gradle-8.13-bin.zip`, `gradle-9.5.0-milestone-7-all.zip`, and
/// percent-encoded values produced by older Wrapper versions.
pub(crate) fn distribution_version(value: &str) -> Option<String> {
    let decoded = value
        .replace("%3A", ":")
        .replace("%2F", "/")
        .replace("%20", " ");
    let file_name = decoded
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&decoded)
        .trim();
    let file_name = file_name.strip_suffix(".zip").unwrap_or(file_name);
    if file_name.is_empty() {
        return None;
    }

    normalize_version(file_name).ok()
}

fn read_observed_versions(project: &Path) -> Result<Vec<String>> {
    let root = project.join(".gradle");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Ok(Vec::new()),
    };

    let mut versions = Vec::new();
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(|character: char| character.is_ascii_digit()) {
            continue;
        }
        if normalize_version(&name).is_ok() {
            versions.push(name);
        }
    }

    versions.sort_by(|left, right| match compare_versions(left, right) {
        Ordering::Equal => left.cmp(right),
        ordering => ordering,
    });
    versions.dedup();
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{distribution_version, inspect_project, scan_projects};
    use crate::store::types::ProjectScanOptions;

    fn write_wrapper(project: &std::path::Path, url: &str) {
        let properties = project
            .join("gradle")
            .join("wrapper")
            .join("gradle-wrapper.properties");
        fs::create_dir_all(properties.parent().unwrap()).unwrap();
        fs::write(
            &properties,
            format!("distributionBase=GRADLE_USER_HOME\ndistributionUrl={url}\n"),
        )
        .unwrap();
    }

    #[test]
    fn distribution_version_handles_wrapper_urls() {
        assert_eq!(
            distribution_version("https\\://services.gradle.org/distributions/gradle-8.13-bin.zip")
                .as_deref(),
            Some("8.13")
        );
        assert_eq!(
            distribution_version("https\\://example.test/gradle-9.5.0-milestone-7-all.zip")
                .as_deref(),
            Some("9.5.0-milestone-7")
        );
        assert_eq!(
            distribution_version("https%3A%2F%2Fexample.test%2Fgradle-8.9-bin.zip").as_deref(),
            Some("8.9")
        );
        assert_eq!(distribution_version(""), None);
    }

    #[test]
    fn inspect_project_reads_wrapper_and_observed_versions() {
        let project = tempfile::tempdir().unwrap();
        write_wrapper(
            project.path(),
            "https\\://services.gradle.org/distributions/gradle-8.13-bin.zip",
        );
        fs::create_dir_all(project.path().join(".gradle/8.13")).unwrap();
        fs::create_dir_all(project.path().join(".gradle/9.5.1")).unwrap();
        fs::create_dir_all(project.path().join(".gradle/buildOutputCleanup")).unwrap();

        let usage = inspect_project(project.path()).unwrap();
        assert_eq!(usage.wrapper_version.as_deref(), Some("8.13"));
        assert_eq!(
            usage.observed_versions,
            vec![String::from("8.13"), String::from("9.5.1")]
        );
    }

    #[test]
    fn inspect_project_reports_a_project_without_cache_directories() {
        let project = tempfile::tempdir().unwrap();
        let usage = inspect_project(project.path()).unwrap();
        assert_eq!(usage.wrapper_version, None);
        assert!(usage.observed_versions.is_empty());
    }

    #[test]
    fn scan_projects_finds_nested_projects_and_skips_build_dirs() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("service-a");
        let nested = root.path().join("apps").join("service-b");
        let ignored = root
            .path()
            .join("service-a")
            .join("build")
            .join("generated");

        write_wrapper(
            &first,
            "https\\://services.gradle.org/distributions/gradle-8.13-bin.zip",
        );
        write_wrapper(
            &nested,
            "https\\://services.gradle.org/distributions/gradle-9.5.1-bin.zip",
        );
        write_wrapper(
            &ignored,
            "https\\://services.gradle.org/distributions/gradle-7.6-bin.zip",
        );

        let projects = scan_projects(&ProjectScanOptions {
            roots: vec![root.path().to_path_buf()],
            depth: 4,
        })
        .unwrap();

        let found = projects
            .iter()
            .map(|project| {
                (
                    project
                        .path
                        .strip_prefix(root.path())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    project.wrapper_version.clone().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            vec![
                (String::from("apps/service-b"), String::from("9.5.1")),
                (String::from("service-a"), String::from("8.13")),
            ]
        );
    }

    #[test]
    fn scan_projects_respects_the_depth_limit() {
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("a").join("b").join("c");
        write_wrapper(
            &deep,
            "https\\://services.gradle.org/distributions/gradle-8.13-bin.zip",
        );

        let shallow = scan_projects(&ProjectScanOptions {
            roots: vec![root.path().to_path_buf()],
            depth: 2,
        })
        .unwrap();
        assert!(shallow.is_empty());

        let deep_scan = scan_projects(&ProjectScanOptions {
            roots: vec![root.path().to_path_buf()],
            depth: 4,
        })
        .unwrap();
        assert_eq!(deep_scan.len(), 1);
    }
}
