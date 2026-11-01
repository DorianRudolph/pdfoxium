// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Child isolation bounds hangs as well as aborts; always reap a killed child.
use crate::Result;
use std::process::Command;
use std::time::{Duration, Instant};

/// Only explicit document/page API rejection uses this worker exit code.
pub const REJECTED_EXIT_CODE: i32 = 2;
#[derive(Debug)]
pub struct PdfRejected(pub String);
impl std::fmt::Display for PdfRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for PdfRejected {}

#[derive(Debug)]
pub enum WorkerFailure {
    Rejected,
    Crashed(std::process::ExitStatus),
    Failed(std::process::ExitStatus),
    Timeout(Duration),
}
impl std::fmt::Display for WorkerFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected => write!(f, "PDF rejected cleanly by document/page API"),
            Self::Crashed(s) => write!(f, "worker crashed with {s}"),
            Self::Failed(s) => write!(f, "worker exited with {s}"),
            Self::Timeout(t) => write!(f, "worker timed out after {:.3}s", t.as_secs_f64()),
        }
    }
}
impl std::error::Error for WorkerFailure {}

pub fn outcome(result: &Result<()>) -> &'static str {
    match result {
        Ok(()) => "rendered",
        Err(e) => match e.downcast_ref::<WorkerFailure>() {
            Some(WorkerFailure::Rejected) => "rejected",
            Some(WorkerFailure::Crashed(_)) => "crashed",
            Some(WorkerFailure::Timeout(_)) => "timeout",
            _ => "error",
        },
    }
}

pub fn run(command: &mut Command, timeout: Duration) -> Result<()> {
    let mut child = command.spawn()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let failure = match status.code() {
                    Some(REJECTED_EXIT_CODE) => WorkerFailure::Rejected,
                    None | Some(101) => WorkerFailure::Crashed(status),
                    Some(code) if code < 0 => WorkerFailure::Crashed(status),
                    _ => WorkerFailure::Failed(status),
                };
                return Err(failure.into());
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(WorkerFailure::Timeout(timeout).into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn hangs_and_unsuccessful_exits_fail_but_success_passes() {
        assert!(run(
            Command::new("sh").args(["-c", "exit 0"]),
            Duration::from_secs(1)
        )
        .is_ok());
        assert!(run(
            Command::new("sh").args(["-c", "exit 7"]),
            Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string()
        .contains("worker exited"));
        let rejected = run(
            Command::new("sh").args(["-c", "exit 2"]),
            Duration::from_secs(1),
        );
        assert_eq!(outcome(&rejected), "rejected");
        let crashed = run(
            Command::new("sh").args(["-c", "kill -TERM $$"]),
            Duration::from_secs(1),
        );
        assert_eq!(outcome(&crashed), "crashed");
        let start = Instant::now();
        let error = run(
            Command::new("sh").args(["-c", "exec sleep 5"]),
            Duration::from_millis(30),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
