/* stress_page/runner.rs
 *
 * Copyright 2026 Stress Center Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Spawns and supervises `stress-ng` from the (unprivileged) UI process.
//!
//! `stress-ng` forks many worker processes of its own, so `Child::kill()`
//! alone is not enough to stop a run: it only kills the direct child, and
//! the workers keep going as orphans. The child is started in its own
//! session (`setsid`) so `-pgid` addresses the whole group, and Stop sends
//! SIGTERM to the group, waits briefly, then SIGKILL if anything is left.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use gtk::glib;

use super::LogKind;

const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;
const STOP_GRACE_PERIOD: Duration = Duration::from_secs(2);

extern "C" {
    fn setsid() -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Process group id of the currently-running `stress-ng`, or 0 if none is
/// active. Kept as a free-standing global (rather than page state) so it can
/// be reached from application shutdown and the panic hook, neither of which
/// necessarily has a live reference to the `StressPage` widget.
static ACTIVE_PGID: AtomicI32 = AtomicI32::new(0);

pub struct StressConfig {
    pub test_cpu: bool,
    pub test_memory: bool,
    pub workers: u32,
    pub duration_secs: u32,
    pub cpu_method: Option<String>,
    pub verify: bool,
}

impl StressConfig {
    pub fn command_line_preview(&self) -> String {
        self.build_args().join(" ")
    }

    fn build_args(&self) -> Vec<String> {
        let mut args = vec![];

        if self.test_cpu {
            args.push("--cpu".to_string());
            args.push(self.workers.to_string());
            if let Some(method) = &self.cpu_method {
                args.push("--cpu-method".to_string());
                args.push(method.clone());
            }
        }

        if self.test_memory {
            args.push("--vm".to_string());
            args.push(self.workers.to_string());
        }

        args.push("--timeout".to_string());
        args.push(format!("{}s", self.duration_secs));

        if self.verify {
            args.push("--verify".to_string());
        }

        args.push("--metrics-brief".to_string());

        args
    }
}

/// Returns true if `stress-ng` can be found and executed.
pub fn is_available() -> bool {
    Command::new("stress-ng")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Delimiters stress-ng has used, across versions, to introduce the list of
/// valid values for an unrecognized `--cpu-method` choice, e.g.:
/// "option cpu-method choice 'which' not known, choices are: all ackermann ..."
const CPU_METHOD_LIST_DELIMITERS: &[&str] = &["choices are:", "must be one of:"];

/// Queries `stress-ng --cpu-method which` for the list of valid CPU stressor
/// methods. Falls back to just `["all"]` if stress-ng is missing or its
/// output doesn't match a known format.
pub fn cpu_methods() -> Vec<String> {
    let fallback = vec!["all".to_string()];

    let output = match Command::new("stress-ng")
        .arg("--cpu-method")
        .arg("which")
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(_) => return fallback,
    };

    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let lower = combined.to_ascii_lowercase();

    let Some(list_start) = CPU_METHOD_LIST_DELIMITERS
        .iter()
        .find_map(|delim| lower.find(delim).map(|idx| idx + delim.len()))
    else {
        return fallback;
    };

    let methods: Vec<String> = combined[list_start..]
        .split_whitespace()
        .map(|s| s.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-'))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if methods.is_empty() {
        fallback
    } else {
        methods
    }
}

fn is_verify_failure(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("fail:") || lower.contains("failed")
}

/// Spawns `stress-ng` in its own session and streams its stdout/stderr into
/// the Stress page's log pane. Returns as soon as the process is spawned;
/// completion is delivered asynchronously via `StressPage::on_process_exited`.
pub fn spawn(config: StressConfig) -> std::io::Result<()> {
    let mut command = Command::new("stress-ng");
    command.args(config.build_args());
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    unsafe {
        command.pre_exec(|| {
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command.spawn()?;

    // setsid() makes the child the leader of its own new session and
    // process group, so its pid is also its pgid.
    let pgid = child.id() as i32;
    ACTIVE_PGID.store(pgid, Ordering::SeqCst);

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    // Widgets aren't `Send`, so the reader/wait threads below never carry
    // `page` (or a weak ref to it) across the thread boundary. Instead each
    // closure posted to the main loop looks the page up fresh, via
    // `super::current_page()`, once it's actually running there.

    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            glib::idle_add_once(move || {
                if let Some(page) = super::current_page() {
                    page.append_log_line(&line, LogKind::Stdout);
                }
            });
        }
    });

    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            glib::idle_add_once(move || {
                if let Some(page) = super::current_page() {
                    let kind = if is_verify_failure(&line) {
                        LogKind::Fail
                    } else {
                        LogKind::Stderr
                    };
                    page.append_log_line(&line, kind);
                }
            });
        }
    });

    std::thread::spawn(move || {
        let status = child.wait();

        // Only this process's own group, so only clear it if it's still
        // the one we started (a fresh run may have replaced it already).
        let _ =
            ACTIVE_PGID.compare_exchange(pgid, 0, Ordering::SeqCst, Ordering::SeqCst);

        glib::idle_add_once(move || {
            if let Some(page) = super::current_page() {
                page.on_process_exited(status);
            }
        });
    });

    Ok(())
}

/// Stops the currently-running `stress-ng`, if any: SIGTERM to the whole
/// process group, then SIGKILL after a grace period if it's still around.
/// Returns immediately; the kill-after-grace-period wait happens on a
/// background thread so the UI is never blocked on it.
pub fn stop() {
    let pgid = ACTIVE_PGID.load(Ordering::SeqCst);
    if pgid == 0 {
        return;
    }

    unsafe {
        kill(-pgid, SIGTERM);
    }

    std::thread::spawn(move || {
        std::thread::sleep(STOP_GRACE_PERIOD);
        if ACTIVE_PGID.load(Ordering::SeqCst) == pgid {
            unsafe {
                kill(-pgid, SIGKILL);
            }
        }
    });
}

/// Immediately (non-gracefully) kills any active `stress-ng` process group.
/// Used from application shutdown and the panic hook, where we can't afford
/// to wait around for a graceful SIGTERM window.
pub fn kill_active_group_now() {
    let pgid = ACTIVE_PGID.swap(0, Ordering::SeqCst);
    if pgid != 0 {
        unsafe {
            kill(-pgid, SIGTERM);
            kill(-pgid, SIGKILL);
        }
    }
}
