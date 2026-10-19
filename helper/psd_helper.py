"""Ddsnap psd-tools helper.

Runs as `python -I -c <this script>`. Reads one JSON request per line on stdin and
answers one JSON line on stdout; a `render` answer is followed by `bytes` raw RGBA bytes.

Requests:
  {"cmd": "open", "path": "/abs/file.psd"}
  {"cmd": "render", "visible": [0, 2, 5], "scale": 0.25}   # indices from the `open` answer
  {"cmd": "quit"}

A layer is drawn only if it and every group above it are listed in `visible`.
Errors never end the process: {"ok": false, "kind": "...", "error": "..."}.
"""

import json
import os
import sys

# stdout belongs to the protocol. Anything else printed later (by us, psd-tools, numpy,
# or C code writing to fd 1) goes to stderr instead of corrupting the byte stream.
_proto = os.fdopen(os.dup(1), "wb")
os.dup2(2, 1)
sys.stdout = sys.stderr

from PIL import Image  # noqa: E402
from psd_tools import PSDImage  # noqa: E402


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
                }
            )
        return {
            "width": psd.width,
            "height": psd.height,
            "depth": psd.depth,
            "color_mode": psd.color_mode.name,
            "layers": tree,
        }, None

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


def reply(header, data=None):
    _proto.write(json.dumps(header, ensure_ascii=False).encode() + b"\n")
    if data is not None:
        _proto.write(data)
    _proto.flush()


def main():
    session = Session()
    handlers = {"open": session.open, "render": session.render}
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
