//! GPU preview (ADR-005): each layer is extracted once at preview scale, then GSK fuses the
//! checked ones on every change. Close to Photoshop, not identical: modes GSK lacks fall back
//! to normal, and clipping uses the base layer's alpha.

use std::collections::HashMap;

use ddsnap::core::helper::Rgba;
use ddsnap::core::layers::LayerTree;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

/// A layer extracted for the preview, placed on the scaled canvas.
pub struct LayerTexture {
    pub x: f32,
    pub y: f32,
    pub texture: gdk::Texture,
}

pub fn texture(image: Rgba) -> gdk::Texture {
    let stride = image.width as usize * 4;
    gdk::MemoryTexture::new(
        image.width as i32,
        image.height as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(image.data),
        stride,
    )
    .upcast()
}

/// Straight (not premultiplied) RGBA, as `core::dds` expects.
pub fn download(texture: &gdk::Texture) -> Rgba {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let data = bytes
        .chunks(stride)
        .take(height)
        .flat_map(|row| &row[..width * 4])
        .copied()
        .collect();
    Rgba {
        width: width as u32,
        height: height as u32,
        data,
    }
}

/// psd-tools blend mode name → GSK; modes GSK lacks (linear dodge, vivid light…) → normal.
pub fn blend_mode(name: &str) -> gsk::BlendMode {
    use gsk::BlendMode::*;
    match name {
        "multiply" => Multiply,
        "screen" => Screen,
        "overlay" => Overlay,
        "darken" => Darken,
        "lighten" => Lighten,
        "color_dodge" => ColorDodge,
        "color_burn" => ColorBurn,
        "hard_light" => HardLight,
        "soft_light" => SoftLight,
        "difference" => Difference,
        "exclusion" => Exclusion,
        "color" => Color,
        "hue" => Hue,
        "saturation" => Saturation,
        "luminosity" => Luminosity,
        _ => Default,
    }
}

pub fn blend_supported(name: &str) -> bool {
    matches!(name, "normal" | "pass_through") || blend_mode(name) != gsk::BlendMode::Default
}

/// Fuses the checked children of `parent` over `below`, bottom to top (helper order).
pub fn compose(
    tree: &LayerTree,
    checked: &[bool],
    textures: &HashMap<usize, Option<LayerTexture>>,
    parent: Option<usize>,
    below: Option<gsk::RenderNode>,
) -> Option<gsk::RenderNode> {
    let mut acc = below;
    // Clipped layers are drawn inside the alpha of the last unclipped layer under them.
    let mut base: Option<gsk::RenderNode> = None;
    for layer in tree.layers.iter().filter(|l| l.parent == parent) {
        if !layer.clipping {
            base = None;
        }
        if !checked[layer.index] || (layer.clipping && base.is_none()) {
            continue;
        }
        let node = if layer.is_group() {
            if layer.blend == "pass_through" && layer.opacity == u8::MAX {
                // The children blend with what is below the group, as in Photoshop.
                acc = compose(tree, checked, textures, Some(layer.index), acc);
                continue;
            }
            let Some(group) = compose(tree, checked, textures, Some(layer.index), None) else {
                continue;
            };
            if layer.opacity < u8::MAX {
                gsk::OpacityNode::new(&group, f32::from(layer.opacity) / 255.0).upcast()
            } else {
                group
            }
        } else {
            // The helper already applied the layer's own opacity, mask and effects.
            let Some(Some(t)) = textures.get(&layer.index) else {
                continue;
            };
            let bounds = graphene::Rect::new(
                t.x,
                t.y,
                t.texture.width() as f32,
                t.texture.height() as f32,
            );
            gsk::TextureNode::new(&t.texture, &bounds).upcast()
        };
        let node = match &base {
            Some(base) if layer.clipping => {
                gsk::MaskNode::new(&node, base, gsk::MaskMode::Alpha).upcast()
            }
            _ => {
                base = Some(node.clone());
                node
            }
        };
        acc = Some(match (acc, blend_mode(&layer.blend)) {
            (None, _) => node,
            (Some(under), gsk::BlendMode::Default) => {
                gsk::ContainerNode::new(&[under, node]).upcast()
            }
            (Some(under), mode) => gsk::BlendNode::new(&under, &node, mode).upcast(),
        });
    }
    acc
}

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    /// Colors over a checkerboard.
    Rgba,
    /// Colors, alpha ignored.
    Rgb,
    /// Alpha as gray.
    Alpha,
}

impl View {
    pub const LABELS: [&str; 3] = ["RVB+A", "RVB", "Alpha"];
    const ALL: [View; 3] = [View::Rgba, View::Rgb, View::Alpha];

    pub fn from_index(i: u32) -> View {
        View::ALL.get(i as usize).copied().unwrap_or(View::Rgba)
    }
}

/// `image` as shown for `view`. GSK applies the color matrix transposed:
/// out[j] = Σ m[i·4 + j] · in[i] + offset[j], with in = (r, g, b, a).
pub fn view_node(image: &gdk::Texture, view: View) -> gsk::RenderNode {
    let bounds = graphene::Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32);
    let node: gsk::RenderNode = gsk::TextureNode::new(image, &bounds).upcast();
    let mut m = [0.0f32; 16];
    match view {
        View::Rgba => return gsk::ContainerNode::new(&[checker(&bounds), node]).upcast(),
        View::Rgb => (m[0], m[5], m[10]) = (1.0, 1.0, 1.0),
        View::Alpha => (m[12], m[13], m[14]) = (1.0, 1.0, 1.0),
    }
    gsk::ColorMatrixNode::new(
        &node,
        &graphene::Matrix::from_float(m),
        &graphene::Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
    .upcast()
}

fn checker(bounds: &graphene::Rect) -> gsk::RenderNode {
    let light = gdk::RGBA::new(0.80, 0.80, 0.80, 1.0);
    let dark = gdk::RGBA::new(0.60, 0.60, 0.60, 1.0);
    let tile_bounds = graphene::Rect::new(0.0, 0.0, 32.0, 32.0);
    let tile = gsk::ContainerNode::new(&[
        gsk::ColorNode::new(&light, &tile_bounds).upcast(),
        gsk::ColorNode::new(&dark, &graphene::Rect::new(0.0, 0.0, 16.0, 16.0)).upcast(),
        gsk::ColorNode::new(&dark, &graphene::Rect::new(16.0, 16.0, 16.0, 16.0)).upcast(),
    ]);
    gsk::RepeatNode::new(bounds, &tile, Some(&tile_bounds)).upcast()
}
