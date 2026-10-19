//! Client of the psd-tools helper (`helper/psd_helper.py`), see ADR-001 and ADR-002.
//!
//! Blocking on purpose: the CLI calls it directly, the GUI from a worker thread.

use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Deserialize;
use serde_json::json;

use super::layers::LayerTree;

const SCRIPT: &str = include_str!("../../helper/psd_helper.py");
const SETUP_HINT: &str = "psd-tools introuvable : lancez scripts/setup-venv.sh";
/// Keeps the last bytes of the helper's stderr for error reports.
const STDERR_TAIL: usize = 16 * 1024;

#[derive(Debug)]
pub enum HelperError {
    PythonMissing(PathBuf),
    PsdToolsMissing,
    /// The helper exited or broke the protocol; carries the end of its stderr.
    HelperCrashed(String),
    /// The helper refused the request (not a PSD, file missing…).
    Psd(String),
}

impl fmt::Display for HelperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PythonMissing(p) => write!(f, "{SETUP_HINT} ({} absent)", p.display()),
            Self::PsdToolsMissing => f.write_str(SETUP_HINT),
            Self::HelperCrashed(stderr) => {
                write!(f, "l'assistant psd-tools s'est arrêté")?;
                if !stderr.is_empty() {
                    write!(f, " :\n{stderr}")?;
                }
                Ok(())
            }
            Self::Psd(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for HelperError {}

pub type Result<T> = std::result::Result<T, HelperError>;

/// Python of the venv created by `scripts/setup-venv.sh` (same lookup order).
pub fn default_python() -> PathBuf {
    let venv = std::env::var_os("DDSNAP_VENV")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let data = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
                });
            data.join("ddsnap/venv")
        });
    venv.join("bin/python")
}

#[derive(Debug, Deserialize)]
struct Failure {
    error: String,
}

#[derive(Debug, Deserialize)]
struct RenderHeader {
    width: u32,
    height: u32,
    bytes: usize,
}

/// An RGBA8 image, rows top to bottom.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Parses one reply line: `Ok(T)` on `"ok": true`, `HelperError::Psd` on `"ok": false`.
fn parse_reply<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| HelperError::HelperCrashed(format!("réponse illisible ({e}) : {line}")))?;
    let decoded = if value["ok"] == true {
        T::deserialize(&value).map(Ok)
    } else {
        Failure::deserialize(&value).map(|f| Err(HelperError::Psd(f.error)))
    };
    decoded.map_err(|e| HelperError::HelperCrashed(format!("réponse inattendue ({e}) : {line}")))?
}

pub struct Helper {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_thread: Option<thread::JoinHandle<()>>,
}

impl Helper {
    pub fn spawn() -> Result<Self> {
        Self::spawn_with(&default_python())
    }

    pub fn spawn_with(python: &Path) -> Result<Self> {
        let mut child = Command::new(python)
            .args(["-I", "-c", SCRIPT])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                io::ErrorKind::NotFound => HelperError::PythonMissing(python.to_path_buf()),
                _ => HelperError::HelperCrashed(e.to_string()),
            })?;
        let (Some(stdin), Some(stdout), Some(mut err)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(HelperError::HelperCrashed("pipes absents".into()));
        };
        // Drained continuously so a chatty helper never blocks on a full pipe.
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&stderr);
        let stderr_thread = thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(n @ 1..) = err.read(&mut buf) {
                let Ok(mut tail) = sink.lock() else { break };
                tail.extend_from_slice(&buf[..n]);
                let excess = tail.len().saturating_sub(STDERR_TAIL);
                tail.drain(..excess);
            }
        });
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr,
            stderr_thread: Some(stderr_thread),
        })
    }

    /// End of the helper's stderr (Python warnings, tracebacks).
    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .map(|t| String::from_utf8_lossy(&t).into_owned())
            .unwrap_or_default()
    }

    fn crashed(&mut self) -> HelperError {
        // The stderr thread ends at EOF, i.e. once the process is gone.
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.stderr_thread.take() {
            let _ = t.join();
        }
        let stderr = self.stderr_tail();
        if stderr.contains("No module named 'psd_tools'")
            || stderr.contains("No module named 'PIL'")
        {
            HelperError::PsdToolsMissing
        } else {
            HelperError::HelperCrashed(stderr)
        }
    }

    fn request(&mut self, req: serde_json::Value) -> Result<String> {
        let mut line = req.to_string();
        line.push('\n');
        if self.stdin.write_all(line.as_bytes()).is_err() || self.stdin.flush().is_err() {
            return Err(self.crashed());
        }
        let mut reply = String::new();
        match self.stdout.read_line(&mut reply) {
            Ok(1..) => Ok(reply),
            _ => Err(self.crashed()),
        }
    }

    /// No timeout: large PSDs are legitimately slow; a dead helper ends the stream.
    pub fn open(&mut self, psd: &Path) -> Result<LayerTree> {
        let path = std::path::absolute(psd).unwrap_or_else(|_| psd.to_path_buf());
        let reply = self.request(json!({"cmd": "open", "path": path}))?;
        parse_reply(&reply)
    }

    /// Composites the layers whose indices are listed (and whose groups are all listed).
    pub fn render(&mut self, visible: &[usize], scale: f64) -> Result<Rgba> {
        let reply = self.request(json!({"cmd": "render", "visible": visible, "scale": scale}))?;
        let header: RenderHeader = parse_reply(&reply)?;
        let mut data = vec![0; header.bytes];
        if self.stdout.read_exact(&mut data).is_err() {
            return Err(self.crashed());
        }
        Ok(Rgba {
            width: header.width,
            height: header.height,
            data,
        })
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(b"{\"cmd\":\"quit\"}\n");
        let _ = self.stdin.flush();
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ok_reply() {
        let tree: LayerTree =
            parse_reply(include_str!("../../tests/fixtures/open_reply.json")).unwrap();
        assert_eq!(tree.layers.len(), 4);
    }

    #[test]
    fn helper_error_becomes_psd_error() {
        let err = parse_reply::<LayerTree>(include_str!("../../tests/fixtures/error_reply.json"))
            .unwrap_err();
        assert!(matches!(&err, HelperError::Psd(m) if m == "fichier introuvable : /nope.psd"));
    }

    #[test]
    fn garbage_reply_is_a_crash() {
        let err = parse_reply::<LayerTree>("parasite").unwrap_err();
        assert!(matches!(err, HelperError::HelperCrashed(_)));
    }

    #[test]
    fn missing_python_says_what_to_do() {
        let Err(err) = Helper::spawn_with(Path::new("/nonexistent/venv/bin/python")) else {
            panic!("spawn should fail");
        };
        assert!(matches!(err, HelperError::PythonMissing(_)));
        assert!(err.to_string().starts_with(SETUP_HINT));
    }

    #[test]
    fn python_without_psd_tools_is_reported() {
        // Ubuntu's system python3 has no psd-tools (PEP 668 forbids installing it there).
        let has_psd_tools = Command::new("python3")
            .args(["-c", "import psd_tools"])
            .status()
            .is_ok_and(|s| s.success());
        if has_psd_tools {
            return;
        }
        let mut h = Helper::spawn_with(Path::new("python3")).unwrap();
        let err = h.open(Path::new("/x.psd")).unwrap_err();
        assert!(matches!(err, HelperError::PsdToolsMissing), "{err:?}");
    }
}
