use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::store::{
    AddMode, AddStatus, CleanSummary, ListRemoteOptions, ManagedVersion, RemoteVersionInfo,
    RemoveStatus, Store,
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
    /// Switch the active Gradle version by updating ~/.gvm/current.
    Use { version: String },
    /// Print the currently selected Gradle version.
    Current,
}

/// Parse arguments, execute the selected command, and print user-facing output.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(None)?;

    match cli.command {
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
        Command::Use { version } => {
            let version = store.use_version(&version)?;
            println!("using {version}");
        }
        Command::Current => match store.current_version()? {
            Some(version) => println!("{version}"),
            None => println!("no version selected"),
        },
    }

    Ok(())
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

fn entry_word(count: usize) -> &'static str {
    if count == 1 { "entry" } else { "entries" }
}
