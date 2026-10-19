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

fn decode(path: &Path) -> ddsnap::core::helper::Rgba {
    ddsnap::core::dds::decode(&std::fs::read(path).unwrap()).unwrap()
}

fn render_psd_visible(psd: &Path) -> ddsnap::core::helper::Rgba {
    let mut helper = ddsnap::core::helper::Helper::spawn().unwrap();
    let tree = helper.open(psd).unwrap();
    helper.render(&tree.psd_visible(), 1.0).unwrap()
}

fn mean_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| f64::from(x.abs_diff(*y)))
        .sum::<f64>()
        / a.len() as f64
}

#[test]
#[ignore]
fn export_matches_render_per_format() {
    let dir = temp_dir("export");
    let psd = make_psd(&dir);
    let expected = render_psd_visible(&psd);
    for (format, max_mean) in [("bc1", 2.0), ("bc3", 2.0), ("bc7", 1.0), ("rgba8", 0.0)] {
        let dest = dir.join(format!("{format}.dds"));
        let out = ddsnap(&[
            "export".as_ref(),
            psd.as_os_str(),
            "--format".as_ref(),
            format.as_ref(),
            "-o".as_ref(),
            dest.as_os_str(),
        ]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = decode(&dest);
        assert_eq!((got.width, got.height), (64, 64));
        let diff = mean_diff(&expected.data, &got.data);
        assert!(diff <= max_mean, "{format}: écart moyen {diff}");
    }
}

#[test]
#[ignore]
fn export_defaults_next_to_psd_and_refuses_psd_as_output() {
    let dir = temp_dir("export-default");
    let psd = make_psd(&dir);
    let out = ddsnap(&["export".as_ref(), psd.as_os_str()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("skin.dds — 64×64 · BC1")
    );
    let before = std::fs::read(&psd).unwrap();
    let out = ddsnap(&[
        "export".as_ref(),
        psd.as_os_str(),
        "-o".as_ref(),
        psd.as_os_str(),
    ]);
    assert!(!out.status.success());
    assert_eq!(std::fs::read(&psd).unwrap(), before);
}
