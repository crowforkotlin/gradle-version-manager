use std::process::Command;

#[cfg(not(unix))]
use std::io;

use anyhow::{Context, Result, bail};

use super::types::{DaemonKind, DaemonProcess, StopOutcome, StopSummary};

/// Marker present in the command line of a Gradle daemon.
const GRADLE_DAEMON_MARKER: &str = "org.gradle.launcher.daemon.bootstrap.GradleDaemon";
/// Marker present in the command line of a Kotlin compile daemon.
const KOTLIN_DAEMON_MARKER: &str = "org.jetbrains.kotlin.daemon.KotlinCompileDaemon";

/// List every Gradle daemon and Kotlin compile daemon currently running.
pub fn list_daemons() -> Result<Vec<DaemonProcess>> {
    let current_pid = std::process::id();
    let mut processes: Vec<DaemonProcess> = raw_process_list()?
        .into_iter()
        .filter(|(pid, _)| *pid != current_pid)
        .filter_map(|(pid, command)| {
            classify_command(&command).map(|kind| DaemonProcess { pid, kind, command })
        })
        .collect();

    processes.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    processes.dedup_by(|left, right| left.pid == right.pid);
    Ok(processes)
}

/// Terminate the selected daemons and report each outcome.
pub fn stop_daemons(kinds: &[DaemonKind], dry_run: bool) -> Result<StopSummary> {
    let daemons = list_daemons()?;
    let selected: Vec<DaemonProcess> = daemons
        .into_iter()
        .filter(|daemon| kinds.contains(&daemon.kind))
        .collect();

    if dry_run {
        return Ok(StopSummary {
            outcomes: selected
                .into_iter()
                .map(|process| StopOutcome {
                    process,
                    stopped: false,
                    detail: None,
                })
                .collect(),
            dry_run: true,
        });
    }

    let mut outcomes = Vec::with_capacity(selected.len());
    for process in selected {
        let outcome = match terminate(process.pid) {
            Ok(()) => StopOutcome {
                process,
                stopped: true,
                detail: None,
            },
            Err(error) => StopOutcome {
                process,
                stopped: false,
                detail: Some(format!("{error:#}")),
            },
        };
        outcomes.push(outcome);
    }

    Ok(StopSummary {
        outcomes,
        dry_run: false,
    })
}

/// Classify a full command line as a Gradle or Kotlin daemon.
pub(super) fn classify_command(command: &str) -> Option<DaemonKind> {
    if command.contains(GRADLE_DAEMON_MARKER) {
        return Some(DaemonKind::Gradle);
    }
    if command.contains(KOTLIN_DAEMON_MARKER) {
        return Some(DaemonKind::Kotlin);
    }
    None
}

/// Parse `ps -A -o pid= -o args=` output into `(pid, command line)` pairs.
pub(super) fn parse_ps_output(output: &str) -> Vec<(u32, String)> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let mut parts = line.splitn(2, char::is_whitespace);
            let pid = parts.next()?.trim().parse::<u32>().ok()?;
            let command = parts.next().unwrap_or_default().trim();
            if command.is_empty() {
                return None;
            }
            Some((pid, command.to_owned()))
        })
        .collect()
}

#[cfg(unix)]
fn raw_process_list() -> Result<Vec<(u32, String)>> {
    let output = Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "args="])
        .output()
        .context("run ps to list processes")?;
    if !output.status.success() {
        bail!("ps exited with {}", output.status);
    }

    Ok(parse_ps_output(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(windows)]
fn raw_process_list() -> Result<Vec<(u32, String)>> {
    let script = "Get-CimInstance Win32_Process | ForEach-Object { \
                  \"$($_.ProcessId)`t$($_.CommandLine)\" }";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .context("run powershell to list processes")?;
    if !output.status.success() {
        bail!("powershell exited with {}", output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.split_once('\t')?;
            let command = command.trim();
            if command.is_empty() {
                return None;
            }
            Some((pid.trim().parse::<u32>().ok()?, command.to_owned()))
        })
        .collect())
}

#[cfg(unix)]
fn terminate(pid: u32) -> Result<()> {
    send_signal(pid, "-TERM")?;
    if wait_for_exit(pid, 20, 250) {
        return Ok(());
    }

    send_signal(pid, "-KILL")?;
    if wait_for_exit(pid, 20, 100) {
        return Ok(());
    }
    bail!("process {pid} did not exit after SIGKILL")
}

#[cfg(windows)]
fn terminate(pid: u32) -> Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .context("run taskkill")?;
    if !status.success() {
        bail!("taskkill exited with {status}");
    }
    Ok(())
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: &str) -> Result<()> {
    let status = Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .with_context(|| format!("run kill {signal} {pid}"))?;
    if !status.success() {
        // ESRCH means the process already exited, which is the desired end state.
        if process_exists(pid) {
            bail!("kill {signal} {pid} exited with {status}");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn wait_for_exit(pid: u32, attempts: u32, delay_ms: u64) -> bool {
    for _ in 0..attempts {
        if !process_exists(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }
    !process_exists(pid)
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    // A zombie still answers `kill -0`, so read the process state and treat a
    // terminated but unreaped entry as gone.
    let output = match Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };
    let state = String::from_utf8_lossy(&output.stdout);
    let state = state.trim();
    !state.is_empty() && !state.starts_with('Z')
}

#[cfg(not(unix))]
fn process_exists(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()),
        Err(error) => !matches!(error.kind(), io::ErrorKind::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_command, parse_ps_output};
    use crate::store::types::DaemonKind;

    #[test]
    fn classify_command_recognizes_gradle_and_kotlin_daemons() {
        let gradle = "/usr/lib/jvm/java/bin/java -cp /x/gradle-launcher-9.5.jar \
                      org.gradle.launcher.daemon.bootstrap.GradleDaemon 9.5";
        let kotlin = "/usr/lib/jvm/java/bin/java -cp /x/kotlin-daemon.jar \
                      org.jetbrains.kotlin.daemon.KotlinCompileDaemon --daemon-runFilesPath /x";
        let unrelated = "/usr/lib/jvm/java/bin/java -jar app.jar";

        assert_eq!(classify_command(gradle), Some(DaemonKind::Gradle));
        assert_eq!(classify_command(kotlin), Some(DaemonKind::Kotlin));
        assert_eq!(classify_command(unrelated), None);
    }

    #[test]
    fn parse_ps_output_skips_blank_and_malformed_lines() {
        let output = "  101 /usr/bin/java org.gradle.launcher.daemon.bootstrap.GradleDaemon\n\
                      \n\
                      not-a-pid command\n\
                      202 /usr/bin/java org.jetbrains.kotlin.daemon.KotlinCompileDaemon\n";

        let parsed = parse_ps_output(output);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, 101);
        assert_eq!(
            parsed[0].1,
            "/usr/bin/java org.gradle.launcher.daemon.bootstrap.GradleDaemon"
        );
        assert_eq!(parsed[1].0, 202);
    }

    #[cfg(unix)]
    #[test]
    fn terminate_stops_a_running_process() {
        use std::process::{Command, Stdio};

        let child = Command::new("sleep")
            .arg("120")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        assert!(super::process_exists(pid));
        super::terminate(pid).unwrap();
        assert!(!super::process_exists(pid));

        // The test owns the handle; wait so the zombie is reaped.
        let mut child = child;
        assert!(!child.wait().unwrap().success());
    }
}
