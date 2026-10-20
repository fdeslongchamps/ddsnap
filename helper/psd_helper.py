"""Ddsnap psd-tools helper.

Runs as `python -I -c <this script>`. Reads one JSON request per line on stdin and
answers one JSON line on stdout; a `render` answer is followed by `bytes` raw RGBA bytes.

Requests:
  {"cmd": "open", "path": "/abs/file.psd"}
  {"cmd": "render", "visible": [0, 2, 5], "scale": 0.25}   # indices from the `open` answer
  {"cmd": "saved"}   # the flattened image Photoshop stored in the file (same answer as render)
  {"cmd": "layer", "index": 3, "scale": 0.25}   # one layer alone (effects, mask, opacity
      applied), for the GPU preview: {"x", "y", "width", "height", "bytes"} + RGBA bytes
  {"cmd": "to_psd", "width": 4, "height": 2, "bytes": 32} + that many raw RGBA bytes on stdin:
      a one-layer PSD of that image, answered as {"bytes"} + the PSD file bytes
  {"cmd": "quit"}

A layer is drawn only if it and every group above it are listed in `visible`.
Errors never end the process: {"ok": false, "kind": "...", "error": "..."}.
"""

import io
import json
import os
import sys

# stdout belongs to the protocol. Anything else printed later (by us, psd-tools, numpy,
# or C code writing to fd 1) goes to stderr instead of corrupting the byte stream.
_proto = os.fdopen(os.dup(1), "wb")
os.dup2(2, 1)
sys.stdout = sys.stderr

from PIL import Image, ImageChops  # noqa: E402
from psd_tools import PSDImage  # noqa: E402
from psd_tools.api.layers import PixelLayer  # noqa: E402


class RequestError(Exception):
    def __init__(self, kind, message):
        super().__init__(message)
        self.kind = kind


class Session:
    def __init__(self):
        self.psd = None
        self.layers = []

    def open(self, req):
        path = req.get("path")
        if not isinstance(path, str):
            raise RequestError("protocol", "open: 'path' manquant")
        if not os.path.isfile(path):
            raise RequestError("not_found", f"fichier introuvable : {path}")
        try:
            psd = PSDImage.open(path)
        except Exception as e:  # psd-tools raises various errors on non-PSD input
            raise RequestError("invalid_psd", f"pas un PSD lisible : {e}") from e
        self.psd = psd
        self.layers = list(psd.descendants())
        index_of = {id(layer): i for i, layer in enumerate(self.layers)}
        tree = []
        for i, layer in enumerate(self.layers):
            path_names = []
            node = layer
            while node is not psd:
                path_names.append(node.name)
                node = node.parent
            path_names.reverse()
            parent = layer.parent
            tree.append(
                {
                    "index": i,
                    "id": layer.layer_id if layer.layer_id >= 0 else None,
                    "name": layer.name,
                    "path": path_names,
                    "parent": None if parent is psd else index_of[id(parent)],
                    "kind": layer.kind,
                    "visible": layer.visible,
                    "has_effects": bool(layer.has_effects()),
                    "opacity": layer.opacity,
                    "blend": layer.blend_mode.name.lower(),
                    "clipping": bool(layer.clipping),
                }
            )
        return {
            "width": psd.width,
            "height": psd.height,
            "depth": psd.depth,
            "color_mode": psd.color_mode.name,
            "layers": tree,
        }, None

    def saved(self, req):
        if self.psd is None:
            raise RequestError("protocol", "saved: aucun PSD ouvert")
        # False when Photoshop saved without "Maximize compatibility": the stored image is blank.
        if not self.psd.has_preview():
            raise RequestError("no_composite", "pas d'image aplatie enregistrée dans le PSD")
        data = self.psd.topil().convert("RGBA")
        return {"width": data.width, "height": data.height, "bytes": data.width * data.height * 4}, data.tobytes()

    def layer(self, req):
        if self.psd is None:
            raise RequestError("protocol", "layer: aucun PSD ouvert")
        index, scale = req.get("index"), req.get("scale", 1.0)
        if not isinstance(index, int) or not 0 <= index < len(self.layers):
            raise RequestError("protocol", "layer: 'index' invalide")
        if not isinstance(scale, (int, float)) or not 0 < scale <= 1:
            raise RequestError("protocol", "layer: 'scale' doit être dans ]0, 1]")
        layer = self.layers[index]
        if layer.is_group():
            raise RequestError("protocol", "layer: un groupe n'a pas d'image propre")
        left, top = layer.bbox[0], layer.bbox[1]
        image = plain_pixels(layer)
        if image is None:
            # layer_filter: draw it even when hidden in the PSD; the user decides in the app.
            image = layer.composite(layer_filter=lambda _: True)
        if image is None or image.width == 0 or image.height == 0:
            return {"x": 0, "y": 0, "width": 0, "height": 0, "bytes": 0}, b""
        image = image.convert("RGBA")
        if scale < 1:
            size = (max(1, round(image.width * scale)), max(1, round(image.height * scale)))
            image = image.resize(size, Image.Resampling.BOX)
        data = image.tobytes()
        header = {"x": round(left * scale), "y": round(top * scale), "width": image.width,
                  "height": image.height, "bytes": len(data)}
        return header, data

    def render(self, req):
        if self.psd is None:
            raise RequestError("protocol", "render: aucun PSD ouvert")
        visible = req.get("visible")
        scale = req.get("scale", 1.0)
        if not isinstance(visible, list) or not all(
            isinstance(i, int) and 0 <= i < len(self.layers) for i in visible
        ):
            raise RequestError("protocol", "render: 'visible' doit lister des index valides")
        if not isinstance(scale, (int, float)) or not 0 < scale <= 1:
            raise RequestError("protocol", "render: 'scale' doit être dans ]0, 1]")
        chosen = {id(self.layers[i]) for i in visible}
        image = self.psd.composite(layer_filter=lambda layer: id(layer) in chosen)
        image = image.convert("RGBA")
        if scale < 1:
            size = (max(1, round(image.width * scale)), max(1, round(image.height * scale)))
            image = image.resize(size, Image.Resampling.BOX)
        data = image.tobytes()
        return {"width": image.width, "height": image.height, "bytes": len(data)}, data


def to_psd(req):
    width, height, size = req.get("width"), req.get("height"), req.get("bytes")
    if not all(isinstance(v, int) and v > 0 for v in (width, height)) or size != width * height * 4:
        raise RequestError("protocol", "to_psd: 'width', 'height' ou 'bytes' invalide")
    data = sys.stdin.buffer.read(size)
    if len(data) != size:
        raise RequestError("protocol", "to_psd: image tronquée")
    image = Image.frombytes("RGBA", (width, height), data)
    psd = PSDImage.new("RGBA", image.size)
    # Private in psd-tools 1.25.0 (pinned): stores the composite's alpha, so readers that use
    # the flattened image (Ddsnap's saved-image path, viewers) keep the transparency.
    psd._merged_alpha = True
    psd.append(PixelLayer.frompil(image, psd, "Texture"))
    out = io.BytesIO()
    psd.save(out)
    return {"bytes": out.tell()}, out.getvalue()


def plain_pixels(layer):
    """Fast path for the preview: decoded pixels with raster mask and opacity applied.

    None when psd-tools must compose the layer (effects, vector shapes, fills): that costs
    seconds per layer instead of milliseconds.
    """
    if layer.kind not in ("pixel", "smartobject", "type") or layer.has_effects():
        return None
    if layer.has_vector_mask() or not layer.has_pixels():
        return None
    image = layer.topil()
    if image is None:
        return None
    image = image.convert("RGBA")
    alpha = image.getchannel("A")
    mask = layer.mask
    if mask is not None and not mask.disabled and mask.width and mask.height:
        full = Image.new("L", image.size, mask.background_color)
        full.paste(mask.topil(), (mask.left - layer.left, mask.top - layer.top))
        alpha = ImageChops.multiply(alpha, full)
    if layer.opacity < 255:
        alpha = alpha.point(lambda a: a * layer.opacity // 255)
    image.putalpha(alpha)
    return image


def reply(header, data=None):
    _proto.write(json.dumps(header, ensure_ascii=False).encode() + b"\n")
    if data is not None:
        _proto.write(data)
    _proto.flush()


def main():
    session = Session()
    handlers = {
        "open": session.open,
        "render": session.render,
        "saved": session.saved,
        "layer": session.layer,
        "to_psd": to_psd,
    }
    for line in sys.stdin.buffer:
        if not line.strip():
            continue
        try:
            req = json.loads(line)
            cmd = req.get("cmd") if isinstance(req, dict) else None
            if cmd == "quit":
                reply({"ok": True})
                return
            if cmd not in handlers:
                raise RequestError("protocol", f"commande inconnue : {cmd!r}")
            header, data = handlers[cmd](req)
            reply({"ok": True, **header}, data)
        except RequestError as e:
            reply({"ok": False, "kind": e.kind, "error": str(e)})
        except json.JSONDecodeError as e:
            reply({"ok": False, "kind": "protocol", "error": f"JSON invalide : {e}"})
        except Exception as e:
            reply({"ok": False, "kind": "internal", "error": f"{type(e).__name__}: {e}"})


main()
