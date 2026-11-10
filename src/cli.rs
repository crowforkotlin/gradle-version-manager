use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::store::{
    AddMode, AddStatus, CacheCleanOptions, CacheCleanSummary, CacheEntry, CacheSelector,
    CleanSummary, DaemonKind, ListRemoteOptions, ManagedVersion, ProjectScanOptions, ProjectUsage,
    RemoteVersionInfo, RemoveStatus, StopSummary, Store, UpdateCheck, cache_entries,
    plan_cache_clean, scan_projects, stop_daemons,
};

/// Command-line interface for the Gradle version manager.
#[derive(Debug, Parser)]
#[command(
    name = "gvm",
    version,
    about = "Gradle version manager",
    long_about = "Manage Gradle installs under ~/.gvm with a stable ~/.gvm/bin/gradle launcher."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Supported top-level subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Download and install a Gradle version from the official distribution service.
    Install { version: String },
    /// List downloadable Gradle versions from the official version service.
    #[command(visible_alias = "ls-remote")]
    ListRemote {
        #[arg(
            long,
            help = "Only show versions from a single major line, for example 8"
        )]
        major: Option<u64>,
        #[arg(long, help = "Include RCs, milestones, and nightly builds")]
        all: bool,
    },
    /// Detect existing Gradle installations from PATH, Wrapper caches, SDKMAN, and common prefixes.
    Detect,
    /// Add an existing Gradle home into ~/.gvm.
    #[command(visible_alias = "import")]
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(
            long,
            help = "Register the external Gradle home through a symlink instead of copying it"
        )]
        link: bool,
    },
    /// List managed Gradle versions and mark the active selection.
    List,
    /// Remove a managed Gradle version.
    #[command(visible_alias = "uninstall")]
    Remove {
        version: String,
        #[arg(long, help = "Allow removing the currently selected version")]
        force: bool,
    },
    /// Clean gvm temporary files and optionally Wrapper cache remnants.
    Clean {
        #[arg(long, help = "Also clean partial files under ~/.gradle/wrapper/dists")]
        wrapper_cache: bool,
        #[arg(
            long,
            help = "Clean everything gvm knows how to clean, including wrapper cache remnants"
        )]
        all: bool,
    },
    /// Inspect or delete Gradle caches under the Gradle user home.
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// Stop running Gradle daemons and Kotlin compile daemons.
    Stop {
        #[arg(
            long,
            value_enum,
            value_delimiter = ',',
            default_value = "gradle,kotlin",
            help = "Daemon kinds to stop"
        )]
        kind: Vec<DaemonKind>,
        #[arg(long, help = "Only print the matching processes")]
        dry_run: bool,
    },
    /// List projects below the given directories and the Gradle versions they use.
    Projects {
        #[arg(
            value_name = "PATH",
            help = "Directory to scan; defaults to the current directory"
        )]
        roots: Vec<PathBuf>,
        #[arg(
            long,
            default_value_t = 3,
            help = "Directory depth to scan below each root"
        )]
        depth: usize,
    },
    /// Check for a newer gvm release and replace the running binary.
    Update {
        #[arg(long, help = "Report the available version without installing it")]
        check: bool,
        #[arg(
            long,
            short = 'y',
            help = "Skip the interactive confirmation; required when stdin is not a terminal"
        )]
        yes: bool,
    },
    /// Switch the active Gradle version by updating ~/.gvm/current.
    Use { version: String },
    /// Print the currently selected Gradle version.
    Current,
}

/// Subcommands of `gvm cache`.
#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// Show removable Gradle cache directories with their sizes.
    List {
        #[arg(
            long,
            value_name = "GRADLE_USER_HOME",
            help = "Inspect an explicit Gradle user home instead of the detected one"
        )]
        home: Option<PathBuf>,
    },
    /// Delete Gradle cache directories after printing the exact paths and sizes.
    Clean {
        #[arg(
            long,
            value_enum,
            value_delimiter = ',',
            help = "Cache areas to delete, for example modules,build-cache"
        )]
        selector: Vec<CacheSelector>,
        #[arg(
            long,
            value_name = "VERSION",
            help = "Delete only the cache directory of this Gradle version"
        )]
        version: Vec<String>,
        #[arg(long, help = "Delete every removable Gradle cache area")]
        all: bool,
        #[arg(long, help = "Print what would be deleted without deleting it")]
        dry_run: bool,
        #[arg(
            long,
            short = 'y',
            help = "Skip the interactive confirmation; required when stdin is not a terminal"
        )]
        yes: bool,
        #[arg(
            long,
            value_name = "GRADLE_USER_HOME",
            help = "Operate on an explicit Gradle user home instead of the detected one"
        )]
        home: Option<PathBuf>,
    },
}

/// Parse arguments, execute the selected command, and print user-facing output.
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Cache { command } => run_cache(command),
        Command::Stop { kind, dry_run } => print_stop_summary(stop_daemons(&kind, dry_run)?),
        Command::Projects { roots, depth } => {
            let projects = scan_projects(&ProjectScanOptions { roots, depth })?;
            print_projects(&projects)
        }
        command => run_store_command(command),
    }
}

fn run_store_command(command: Command) -> Result<()> {
    let store = Store::new(None)?;

    match command {
        Command::Install { version } => match store.install_version(&version)? {
            crate::store::InstallStatus::Installed(version) => println!("installed {version}"),
            crate::store::InstallStatus::AlreadyInstalled(version) => {
                println!("{version} is already installed")
            }
        },
        Command::ListRemote { major, all } => {
            let versions = store.list_remote_versions(ListRemoteOptions {
                major,
                include_prerelease: all,
            })?;
            if versions.is_empty() {
                println!("no remote Gradle versions found");
            } else {
                print_remote_versions(&versions)?;
            }
        }
        Command::Detect => {
            let detected = store.detect_versions()?;
            if detected.is_empty() {
                println!("no Gradle installations detected");
            } else {
                let mut stdout = io::BufWriter::new(io::stdout().lock());
                for item in detected {
                    let managed = item
                        .managed_as
                        .as_deref()
                        .map(|version| format!(" [managed as {version}]"))
                        .unwrap_or_default();
                    writeln!(
                        stdout,
                        "{:<10} {:<18} {}{}",
                        item.source.label(),
                        item.version,
                        item.home.display(),
                        managed
                    )
                    .context("write detection result to stdout")?;
                }
            }
        }
        Command::Add { path, name, link } => {
            let mode = if link { AddMode::Link } else { AddMode::Copy };
            match store.add_version(&path, name.as_deref(), mode)? {
                AddStatus::Added(version) => match mode {
                    AddMode::Copy => println!("added {version}"),
                    AddMode::Link => println!("linked {version}"),
                },
                AddStatus::AlreadyManaged(version) => {
                    println!("{version} is already managed")
                }
            }
        }
        Command::List => {
            let versions = store.list_versions()?;
            if versions.is_empty() {
                println!("no Gradle versions installed");
            } else {
                print_managed_versions(&versions)?;
            }
        }
        Command::Remove { version, force } => match store.remove_version(&version, force)? {
            RemoveStatus::Removed(version) => println!("removed {version}"),
            RemoveStatus::NotInstalled(version) => println!("{version} is not installed"),
        },
        Command::Clean { wrapper_cache, all } => {
            print_clean_summary(store.clean(wrapper_cache || all)?)?;
        }
        Command::Update { check, yes } => {
            let available = store.check_update()?;
            print_update_status(&available)?;
            if !check && !available.up_to_date {
                if !yes {
                    confirm_update(&available)?;
                }
                let path = store.apply_update(&available)?;
                println!("updated gvm to {} at {}", available.latest, path.display());
            }
        }
        Command::Use { version } => {
            let version = store.use_version(&version)?;
            println!("using {version}");
        }
        Command::Current => match store.current_version()? {
            Some(version) => println!("{version}"),
            None => println!("no version selected"),
        },
        Command::Cache { .. } | Command::Stop { .. } | Command::Projects { .. } => {
            unreachable!("these commands are dispatched before the store is created")
        }
    }

    Ok(())
}

fn run_cache(command: CacheCommand) -> Result<()> {
    match command {
        CacheCommand::List { home } => {
            let entries = match home {
                Some(home) => crate::store::cache_entries_in(&home)?,
                None => cache_entries()?,
            };
            print_cache_entries(&entries)
        }
        CacheCommand::Clean {
            selector,
            version,
            all,
            dry_run,
            yes,
            home,
        } => {
            let options = CacheCleanOptions {
                selectors: selector,
                versions: version,
                all,
                dry_run,
            };
            // The plan is measured once: deleting a 20 GiB cache twice per run
            // would double the walk for no extra safety.
            let plan = match &home {
                Some(home) => crate::store::plan_cache_clean_in(home, &options)?,
                None => plan_cache_clean(&options)?,
            };
            if plan.removed.is_empty() {
                println!("no Gradle caches matched");
                return Ok(());
            }

            print_cache_preview(&plan)?;
            if dry_run {
                println!("dry run: nothing was deleted");
                return Ok(());
            }

            if !yes {
                confirm_cache_deletion(&plan)?;
            }

            print_cache_summary(&crate::store::apply_cache_clean(&plan)?)
        }
    }
}

fn print_managed_versions(versions: &[ManagedVersion]) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for version in versions {
        let marker = if version.is_current { "* " } else { "  " };
        writeln!(stdout, "{marker}{}", version.name).context("write version list to stdout")?;
    }
    Ok(())
}

fn print_remote_versions(versions: &[RemoteVersionInfo]) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for version in versions {
        let marker = if version.current { "* " } else { "  " };
        writeln!(
            stdout,
            "{marker}{:<18} {}",
            version.version,
            version.channel.label()
        )
        .context("write remote version list to stdout")?;
    }
    Ok(())
}

fn print_clean_summary(summary: CleanSummary) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(
        stdout,
        "removed {} {}",
        summary.gvm_entries,
        entry_word(summary.gvm_entries)
    )
    .context("write clean summary to stdout")?;
    if summary.wrapper_entries > 0 {
        writeln!(
            stdout,
            "removed {} wrapper cache {}",
            summary.wrapper_entries,
            entry_word(summary.wrapper_entries)
        )
        .context("write wrapper clean summary to stdout")?;
    }
    Ok(())
}

fn print_cache_entries(entries: &[CacheEntry]) -> Result<()> {
    if entries.is_empty() {
        println!("no removable Gradle caches found");
        return Ok(());
    }

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(stdout, "{:<14} {:>10} {:>8}  PATH", "AREA", "SIZE", "FILES")
        .context("write cache header to stdout")?;
    for entry in entries {
        writeln!(
            stdout,
            "{:<14} {:>10} {:>8}  {}",
            entry.selector.label(),
            human_bytes(entry.bytes),
            entry.files,
            entry.path.display()
        )
        .context("write cache entry to stdout")?;
    }
    writeln!(
        stdout,
        "total {} across {} {}",
        human_bytes(entries.iter().map(|entry| entry.bytes).sum()),
        entries.len(),
        entry_word(entries.len())
    )
    .context("write cache total to stdout")?;
    Ok(())
}

fn print_cache_preview(summary: &CacheCleanSummary) -> Result<()> {
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(
        stdout,
        "the following Gradle caches will be deleted ({}):",
        human_bytes(summary.freed_bytes)
    )
    .context("write cache preview header to stdout")?;
    for entry in &summary.removed {
        writeln!(
            stdout,
            "  {:<14} {:>10}  {}",
            entry.selector.label(),
            human_bytes(entry.bytes),
            entry.path.display()
        )
        .context("write cache preview entry to stdout")?;
    }
    writeln!(
        stdout,
        "Gradle will re-download dependencies and rebuild artifacts after this deletion."
    )
    .context("write cache warning to stdout")?;
    Ok(())
}

fn print_cache_summary(summary: &CacheCleanSummary) -> Result<()> {
    println!(
        "deleted {} {} ({})",
        summary.removed.len(),
        entry_word(summary.removed.len()),
        human_bytes(summary.freed_bytes)
    );
    Ok(())
}

fn print_stop_summary(summary: StopSummary) -> Result<()> {
    if summary.outcomes.is_empty() {
        println!("no running Gradle or Kotlin daemons");
        return Ok(());
    }

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for outcome in &summary.outcomes {
        let status = if summary.dry_run {
            "running"
        } else if outcome.stopped {
            "stopped"
        } else {
            "failed"
        };
        writeln!(
            stdout,
            "{:<8} {:<7} {:>7}  {}",
            status,
            outcome.process.kind.label(),
            outcome.process.pid,
            truncate_middle(&outcome.process.command, 120)
        )
        .context("write stop result to stdout")?;
        if let Some(detail) = &outcome.detail {
            writeln!(stdout, "         {detail}").context("write stop failure detail")?;
        }
    }
    Ok(())
}

fn print_projects(projects: &[ProjectUsage]) -> Result<()> {
    if projects.is_empty() {
        println!("no Gradle projects found");
        return Ok(());
    }

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for project in projects {
        let wrapper = project.wrapper_version.as_deref().unwrap_or("-");
        let observed = if project.observed_versions.is_empty() {
            String::from("-")
        } else {
            project.observed_versions.join(",")
        };
        writeln!(
            stdout,
            "{:<12} {:<24} {}",
            wrapper,
            observed,
            project.path.display()
        )
        .context("write project row to stdout")?;
    }
    writeln!(
        stdout,
        "wrapper = version in gradle-wrapper.properties, observed = .gradle/<version> directories"
    )
    .context("write project legend to stdout")?;
    Ok(())
}

fn print_update_status(available: &UpdateCheck) -> Result<()> {
    if available.up_to_date {
        println!("gvm {} is up to date", available.current);
    } else {
        println!(
            "gvm {} can be updated to {} ({})",
            available.current, available.latest, available.asset_name
        );
    }
    Ok(())
}

fn confirm_cache_deletion(summary: &CacheCleanSummary) -> Result<()> {
    if !io::stdin().is_terminal() {
        bail!(
            "refusing to delete Gradle caches without confirmation; re-run with --yes to confirm"
        );
    }

    eprint!(
        "Delete these caches and {} of data? [y/N] ",
        human_bytes(summary.freed_bytes)
    );
    io::stderr().flush().context("flush confirmation prompt")?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("read confirmation answer")?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer != "y" && answer != "yes" {
        bail!("aborted; no Gradle caches were deleted");
    }
    Ok(())
}

fn confirm_update(available: &UpdateCheck) -> Result<()> {
    if !io::stdin().is_terminal() {
        bail!(
            "refusing to replace the running gvm binary without confirmation; re-run with --yes-style confirmation in a terminal"
        );
    }

    eprint!(
        "Replace the running gvm {} with {}? [y/N] ",
        available.current, available.latest
    );
    io::stderr()
        .flush()
        .context("flush update confirmation prompt")?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("read confirmation answer")?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer != "y" && answer != "yes" {
        bail!("aborted; gvm was not updated");
    }
    Ok(())
}

fn entry_word(count: usize) -> &'static str {
    if count == 1 { "entry" } else { "entries" }
}

/// Shorten a long command line for display while keeping both ends readable.
fn truncate_middle(text: &str, max_chars: usize) -> String {
    let characters = text.chars().count();
    if characters <= max_chars {
        return text.to_owned();
    }

    let keep = max_chars.saturating_sub(3) / 2;
    let head = text.chars().take(keep).collect::<String>();
    let tail = text.chars().skip(characters - keep).collect::<String>();
    format!("{head}...{tail}")
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
