//! End-to-end tests; need the psd-tools venv (`scripts/setup-venv.sh`).
//! Run with `cargo test -- --ignored`.

use std::path::{Path, PathBuf};
use std::process::Command;

use ddsnap::core::helper::default_python;

fn make_psd(dir: &Path) -> PathBuf {
    let psd = dir.join("skin.psd");
    let status = Command::new(default_python())
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/make_psd.py"
        ))
        .arg(&psd)
        .status()
        .expect("venv python");
    assert!(status.success());
    psd
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ddsnap-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ddsnap(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ddsnap"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
#[ignore]
fn layers_prints_tree() {
    let dir = temp_dir("layers");
    let psd = make_psd(&dir);
    let out = ddsnap(&["layers".as_ref(), psd.as_os_str()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "64×64 · 8 bits · RGB\nRed\nG  [group]\n  Blue/Bleu\nGreen  [masqué dans le PSD]\n"
    );
}
