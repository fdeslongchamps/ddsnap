//! Log file and diagnostic report (`ddsnap doctor`, Aide › Diagnostic).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use super::helper::default_python;

/// Rotated past this size: ddsnap.log → ddsnap.log.1 → ddsnap.log.2, three files at most.
const MAX_LOG: u64 = 5 * 1024 * 1024;
const KEPT: usize = 2;

/// `$XDG_STATE_HOME/ddsnap/ddsnap.log`, default `~/.local/state/ddsnap/ddsnap.log`.
pub fn log_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("ddsnap/ddsnap.log")
}

/// Appends one line to the log file. Never fails the caller: a log that cannot be written
/// must not stop an export.
pub fn log(line: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG) {
        for i in (1..KEPT).rev() {
            let _ = fs::rename(rotated(i), rotated(i + 1));
        }
        let _ = fs::rename(&path, rotated(1));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
}

fn rotated(i: usize) -> PathBuf {
    let mut path = log_path().into_os_string();
    path.push(format!(".{i}"));
    path.into()
}

/// Versions, Python and psd-tools state, OS, and the end of the log: what to paste in a bug
/// report. The first line says what is wrong, if anything.
pub fn report() -> String {
    let python = default_python();
    let psd_tools = Command::new(&python)
        .args(["-I", "-c", "import psd_tools; print(psd_tools.__version__)"])
        .output();
    let (verdict, psd_tools) = match psd_tools {
        Err(_) => (
            format!(
                "PROBLÈME : Python du venv absent ({}). Lancez scripts/setup-venv.sh.",
                python.display()
            ),
            "absent".to_string(),
        ),
        Ok(out) if !out.status.success() => (
            "PROBLÈME : psd-tools n'est pas installé dans le venv. Lancez scripts/setup-venv.sh."
                .to_string(),
            "absent".to_string(),
        ),
        Ok(out) => (
            "OK : Python et psd-tools sont prêts.".to_string(),
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ),
    };
    let os = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    let log = fs::read_to_string(log_path()).unwrap_or_default();
    let tail: Vec<&str> = log.lines().rev().take(40).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    format!(
        "{verdict}\n\nDdsnap {}\nSystème : {os}\nPython : {}\npsd-tools : {psd_tools}\nJournal : {}\n\n--- fin du journal ---\n{}",
        env!("CARGO_PKG_VERSION"),
        python.display(),
        log_path().display(),
        tail.join("\n")
    )
}
