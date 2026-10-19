//! One output end to end: render → DDS → atomic write.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::dds::{self, DdsError, Format, Size};
use super::helper::{Helper, HelperError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub format: Format,
    pub size: Size,
    pub no_mipmaps: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: Format,
    pub bytes: usize,
    pub warnings: Vec<String>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        write!(
            f,
            "{name} — {}×{} · {} · {} Mo",
            self.width,
            self.height,
            self.format.to_string().to_uppercase(),
            format!("{:.1}", self.bytes as f64 / 1e6).replace('.', ",")
        )
    }
}

#[derive(Debug)]
pub enum ExportError {
    Helper(HelperError),
    Dds(DdsError),
    Io(PathBuf, io::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Helper(e) => e.fmt(f),
            Self::Dds(e) => e.fmt(f),
            Self::Io(path, e) => write!(f, "écriture de {} : {e}", path.display()),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<HelperError> for ExportError {
    fn from(e: HelperError) -> Self {
        Self::Helper(e)
    }
}

impl From<DdsError> for ExportError {
    fn from(e: DdsError) -> Self {
        Self::Dds(e)
    }
}

/// Renders `visible` at full size and writes it to `dest` as DDS.
/// `as_saved`: `visible` is the PSD's own visibility, so Photoshop's stored image is used when
/// present (seconds instead of minutes, ADR-005).
/// `step` is called with a French label before each phase (CLI output, GUI progress bar).
pub fn export(
    helper: &mut Helper,
    visible: &[usize],
    as_saved: bool,
    settings: Settings,
    dest: &Path,
    step: &mut dyn FnMut(&str),
) -> Result<Report, ExportError> {
    let saved = if as_saved {
        step("lecture du composite Photoshop enregistré dans le PSD");
        helper.saved()?
    } else {
        None
    };
    let rendered = match saved {
        Some(image) => image,
        None => {
            if as_saved {
                step("pas d'image aplatie (« Maximiser la compatibilité » désactivé) : psd-tools");
            }
            step(&format!(
                "rendu de {} calques (psd-tools, peut prendre plusieurs minutes)",
                visible.len()
            ));
            helper.render(visible, 1.0)?
        }
    };
    let (width, height) = settings.size.target(rendered.width, rendered.height);
    if (width, height) != (rendered.width, rendered.height) {
        step(&format!("redimensionnement en {width}×{height}"));
    }
    let image = dds::resize(rendered, settings.size);
    let format = settings.format.resolve(&image.data);
    let warnings = dds::size_warnings(image.width, image.height, format);
    let mips = if settings.no_mipmaps {
        ""
    } else {
        " + mipmaps"
    };
    step(&format!(
        "encodage {}{mips}",
        format.to_string().to_uppercase()
    ));
    let bytes = dds::encode(&image, format, !settings.no_mipmaps)?;
    step(&format!("écriture de {}", dest.display()));
    write_atomic(dest, |f| f.write_all(&bytes)).map_err(|e| ExportError::Io(dest.into(), e))?;
    Ok(Report {
        path: dest.into(),
        width: image.width,
        height: image.height,
        format,
        bytes: bytes.len(),
        warnings,
    })
}

/// Writes through a temporary file in the same folder, then renames it over `dest`:
/// readers see the old file or the new one, never half of it.
pub fn write_atomic(
    dest: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let name = dest.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = dest.with_file_name(tmp_name);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        write(&mut file)?;
        file.sync_all()?;
        fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ddsnap-unit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn failed_write_leaves_nothing_behind() {
        let dir = temp_dir("atomic-fail");
        let dest = dir.join("livery.dds");
        let err = write_atomic(&dest, |f| {
            f.write_all(b"DDS half")?;
            Err(io::Error::other("disque plein"))
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "disque plein");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_write_keeps_previous_file() {
        let dir = temp_dir("atomic-keep");
        let dest = dir.join("livery.dds");
        fs::write(&dest, b"old").unwrap();
        let _ = write_atomic(&dest, |_| Err(io::Error::other("x")));
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        write_atomic(&dest, |f| f.write_all(b"new")).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_reads_like_the_toast() {
        let report = Report {
            path: "/x/livery.dds".into(),
            width: 4096,
            height: 4096,
            format: Format::Bc3,
            bytes: 22_369_776,
            warnings: vec![],
        };
        assert_eq!(report.to_string(), "livery.dds — 4096×4096 · BC3 · 22,4 Mo");
    }
}
