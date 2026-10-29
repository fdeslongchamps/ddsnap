//! DDS encoding for Assetto Corsa (ADR-003): format choice, resizing, encoding, decoding.

use std::fmt;
use std::str::FromStr;

use image_dds::ddsfile::{D3DFormat, Dds, NewD3dParams};
use image_dds::image::{RgbaImage, imageops};
use image_dds::{ImageFormat, Mipmaps, Quality, SurfaceRgba8};
use serde::{Deserialize, Serialize};

use super::helper::Rgba;

/// Output format; `Auto` is resolved per image by [`Format::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Auto,
    Bc1,
    Bc3,
    Bc7,
    Rgba8,
}

impl Format {
    /// `Auto` → BC1 when every pixel is opaque, BC3 otherwise; other formats unchanged.
    pub fn resolve(self, rgba: &[u8]) -> Format {
        match self {
            Format::Auto if rgba.as_chunks::<4>().0.iter().all(|px| px[3] == u8::MAX) => {
                Format::Bc1
            }
            Format::Auto => Format::Bc3,
            f => f,
        }
    }

    pub fn is_block_compressed(self) -> bool {
        matches!(self, Format::Bc1 | Format::Bc3 | Format::Bc7)
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Auto => "auto",
            Format::Bc1 => "bc1",
            Format::Bc3 => "bc3",
            Format::Bc7 => "bc7",
            Format::Rgba8 => "rgba8",
        })
    }
}

impl FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        serde_json::from_value(serde_json::Value::String(s.to_lowercase()))
            .map_err(|_| format!("format inconnu : {s} (auto, bc1, bc3, bc7, rgba8)"))
    }
}

/// Target size: the PSD size, or its largest side reduced to 4096, 2048 or 1024.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Size {
    #[default]
    Source,
    Max(u32),
}

impl Size {
    /// Never enlarges; keeps the aspect ratio.
    pub fn target(self, width: u32, height: u32) -> (u32, u32) {
        match self {
            Size::Max(max) if width.max(height) > max => {
                let scale = f64::from(max) / f64::from(width.max(height));
                let fit = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
                (fit(width), fit(height))
            }
            _ => (width, height),
        }
    }
}

impl FromStr for Size {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "source" => Ok(Size::Source),
            "4096" | "2048" | "1024" => Ok(Size::Max(s.parse().unwrap_or(4096))),
            _ => Err(format!("taille inconnue : {s} (source, 4096, 2048, 1024)")),
        }
    }
}

impl TryFrom<String> for Size {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<Size> for String {
    fn from(size: Size) -> String {
        match size {
            Size::Source => "source".into(),
            Size::Max(n) => n.to_string(),
        }
    }
}

/// Warnings for sizes AC or BCn handle badly (they never block the export).
pub fn size_warnings(width: u32, height: u32, format: Format) -> Vec<String> {
    let mut warnings = Vec::new();
    if format.is_block_compressed() && (!width.is_multiple_of(4) || !height.is_multiple_of(4)) {
        warnings.push(format!(
            "{width}×{height} n'est pas multiple de 4 : la compression {format} travaille par blocs 4×4"
        ));
    }
    if !width.is_power_of_two() || !height.is_power_of_two() {
        warnings.push(format!(
            "{width}×{height} n'est pas une puissance de 2 : mipmaps imparfaits dans le jeu"
        ));
    }
    warnings
}

pub fn resize(image: Rgba, size: Size) -> Rgba {
    let (width, height) = size.target(image.width, image.height);
    if (width, height) == (image.width, image.height) {
        return image;
    }
    let Some(src) = RgbaImage::from_raw(image.width, image.height, image.data) else {
        unreachable!("Rgba data length is width × height × 4");
    };
    let out = imageops::resize(&src, width, height, imageops::FilterType::Lanczos3);
    Rgba {
        width,
        height,
        data: out.into_raw(),
    }
}

#[derive(Debug)]
pub struct DdsError(pub String);

impl fmt::Display for DdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DdsError {}

fn err(e: impl fmt::Display) -> DdsError {
    DdsError(format!("encodage DDS : {e}"))
}

fn decode_err(e: impl fmt::Display) -> DdsError {
    DdsError(format!("décodage DDS : {e}"))
}

/// Encodes to DDS file bytes. `format` must be resolved (not `Auto`).
///
/// BC1/BC3/RGBA8 get a classic DX9 header (DXT1/DXT5/A8R8G8B8) that every DDS reader knows;
/// BC7 has no DX9 code and gets a DX10 header.
pub fn encode(image: &Rgba, format: Format, mipmaps: bool) -> Result<Vec<u8>, DdsError> {
    let (image_format, d3d) = match format {
        Format::Bc1 => (ImageFormat::BC1RgbaUnorm, Some(D3DFormat::DXT1)),
        Format::Bc3 => (ImageFormat::BC3RgbaUnorm, Some(D3DFormat::DXT5)),
        Format::Rgba8 => (ImageFormat::Bgra8Unorm, Some(D3DFormat::A8R8G8B8)),
        Format::Bc7 => (ImageFormat::BC7RgbaUnorm, None),
        Format::Auto => return Err(DdsError("format Auto non résolu".into())),
    };
    let mipmaps = if mipmaps {
        Mipmaps::GeneratedAutomatic
    } else {
        Mipmaps::Disabled
    };
    let surface = SurfaceRgba8 {
        width: image.width,
        height: image.height,
        depth: 1,
        layers: 1,
        mipmaps: 1,
        data: image.data.as_slice(),
    }
    .encode(image_format, Quality::Normal, mipmaps)
    .map_err(err)?;
    let dds = match d3d {
        Some(format) => {
            let mut dds = Dds::new_d3d(NewD3dParams {
                height: surface.height,
                width: surface.width,
                depth: None,
                format,
                mipmap_levels: (surface.mipmaps > 1).then_some(surface.mipmaps),
                caps2: None,
            })
            .map_err(err)?;
            dds.data = surface.data;
            dds
        }
        None => surface.to_dds().map_err(err)?,
    };
    let mut bytes = Vec::new();
    dds.write(&mut bytes).map_err(err)?;
    Ok(bytes)
}

/// Decodes the full-size level, exactly as the game will sample it.
pub fn decode(bytes: &[u8]) -> Result<Rgba, DdsError> {
    let dds = Dds::read(bytes).map_err(decode_err)?;
    let image = image_dds::image_from_dds(&dds, 0).map_err(decode_err)?;
    Ok(Rgba {
        width: image.width(),
        height: image.height(),
        data: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32, alpha: u8) -> Rgba {
        let data = (0..width * height)
            .flat_map(|i| [(i % width * 4) as u8, (i / width * 4) as u8, 128, alpha])
            .collect();
        Rgba {
            width,
            height,
            data,
        }
    }

    #[test]
    fn auto_picks_bc1_only_when_fully_opaque() {
        assert_eq!(
            Format::Auto.resolve(&[1, 2, 3, 255, 4, 5, 6, 255]),
            Format::Bc1
        );
        assert_eq!(
            Format::Auto.resolve(&[1, 2, 3, 255, 4, 5, 6, 254]),
            Format::Bc3
        );
        assert_eq!(Format::Bc7.resolve(&[0, 0, 0, 0]), Format::Bc7);
    }

    #[test]
    fn parses_cli_and_recipe_values() {
        assert_eq!("BC3".parse(), Ok(Format::Bc3));
        assert!("png".parse::<Format>().is_err());
        assert_eq!("2048".parse(), Ok(Size::Max(2048)));
        assert!("3000".parse::<Size>().is_err());
        assert_eq!(serde_json::to_string(&Size::Source).unwrap(), "\"source\"");
        assert_eq!(
            serde_json::from_str::<Size>("\"1024\"").unwrap(),
            Size::Max(1024)
        );
    }

    #[test]
    fn size_target_only_shrinks_and_keeps_ratio() {
        assert_eq!(Size::Source.target(4096, 2048), (4096, 2048));
        assert_eq!(Size::Max(2048).target(4096, 2048), (2048, 1024));
        assert_eq!(Size::Max(4096).target(1024, 1024), (1024, 1024));
    }

    #[test]
    fn invalid_sizes_warn() {
        assert!(size_warnings(1024, 512, Format::Bc1).is_empty());
        assert_eq!(size_warnings(1000, 1000, Format::Bc3).len(), 1); // ÷4, not 2ⁿ
        assert_eq!(size_warnings(1022, 1024, Format::Bc3).len(), 2);
        assert_eq!(size_warnings(1022, 1024, Format::Rgba8).len(), 1);
    }

    #[test]
    fn resize_shrinks() {
        let out = resize(gradient(64, 32, 255), Size::Max(32));
        assert_eq!(
            (out.width, out.height, out.data.len()),
            (32, 16, 32 * 16 * 4)
        );
    }

    fn header(bytes: &[u8]) -> Dds {
        Dds::read(bytes).unwrap()
    }

    #[test]
    fn dds_headers_match_format() {
        let image = gradient(64, 32, 255);
        for (format, fourcc) in [
            (Format::Bc1, Some(D3DFormat::DXT1)),
            (Format::Bc3, Some(D3DFormat::DXT5)),
            (Format::Rgba8, Some(D3DFormat::A8R8G8B8)),
            (Format::Bc7, None),
        ] {
            let dds = header(&encode(&image, format, true).unwrap());
            assert_eq!((dds.get_width(), dds.get_height()), (64, 32), "{format}");
            assert_eq!(dds.get_num_mipmap_levels(), 7, "{format}"); // 64 → 1
            assert_eq!(dds.get_d3d_format(), fourcc, "{format}");
            if format == Format::Bc7 {
                assert_eq!(
                    dds.get_dxgi_format(),
                    Some(image_dds::ddsfile::DxgiFormat::BC7_UNorm)
                );
            }
        }
        let dds = header(&encode(&image, Format::Bc1, false).unwrap());
        assert_eq!(dds.get_num_mipmap_levels(), 1);
    }

    #[test]
    fn round_trip_stays_close() {
        let image = gradient(64, 64, 200);
        for (format, max_mean) in [(Format::Rgba8, 0.0), (Format::Bc3, 3.0), (Format::Bc7, 2.0)] {
            let back = decode(&encode(&image, format, true).unwrap()).unwrap();
            assert_eq!((back.width, back.height), (64, 64));
            let diff: f64 = image
                .data
                .iter()
                .zip(&back.data)
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>()
                / image.data.len() as f64;
            assert!(diff <= max_mean, "{format}: écart moyen {diff}");
        }
    }

    #[test]
    fn odd_size_still_encodes() {
        let back = decode(&encode(&gradient(6, 6, 255), Format::Bc1, true).unwrap()).unwrap();
        assert_eq!((back.width, back.height), (6, 6));
    }

    #[test]
    fn unreadable_dds_reports_decoding() {
        let e = decode(b"not a dds file").unwrap_err();
        assert!(e.to_string().starts_with("décodage DDS : "), "{e}");
    }

    #[test]
    fn auto_must_be_resolved() {
        assert!(encode(&gradient(4, 4, 255), Format::Auto, false).is_err());
    }
}
