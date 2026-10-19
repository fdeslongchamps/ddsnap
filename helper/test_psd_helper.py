"""Tests for psd_helper.py, run as a real subprocess exactly like Ddsnap runs it."""

import json
import subprocess
import sys
from pathlib import Path

import pytest
from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import Group, PixelLayer
from psd_tools.constants import BlendMode, Resource, Tag
from psd_tools.psd.image_resources import ImageResource, VersionInfo

SCRIPT = (Path(__file__).parent / "psd_helper.py").read_text()

RED = (255, 0, 0, 255)
BLUE = (0, 0, 255, 255)
GREEN = (0, 255, 0, 255)
EMPTY = (255, 255, 255, 0)


class Helper:
    def __init__(self, prefix=""):
        self.proc = subprocess.Popen(
            [sys.executable, "-I", "-c", prefix + SCRIPT],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

    def send(self, req):
        line = req if isinstance(req, str) else json.dumps(req)
        self.proc.stdin.write(line.encode() + b"\n")
        self.proc.stdin.flush()
        header = json.loads(self.proc.stdout.readline())
        data = self.proc.stdout.read(header["bytes"]) if header.get("bytes") else None
        return header, data

    def close(self):
        if self.proc.poll() is None:
            self.send({"cmd": "quit"})
        self.proc.wait(timeout=10)
        self.proc.stdin.close()
        self.proc.stdout.close()
        self.proc.stderr.close()


@pytest.fixture
def helper():
    h = Helper()
    yield h
    h.close()


def set_id(layer, layer_id):
    layer._record.tagged_blocks.set_data(Tag.LAYER_ID, layer_id)


@pytest.fixture
def psd_path(tmp_path):
    """8×8 PSD, bottom to top: Red (full), group G { Blue (top-left 4×4) }, Green (hidden)."""
    psd = PSDImage.new("RGB", (8, 8))
    red = PixelLayer.frompil(Image.new("RGBA", (8, 8), RED), psd, "Red")
    group = Group.new(psd, "G")
    blue = PixelLayer.frompil(Image.new("RGBA", (4, 4), BLUE), group, "Blue/Bleu")
    green = PixelLayer.frompil(Image.new("RGBA", (8, 8), GREEN), psd, "Green")
    green.visible = False
    for layer, layer_id in [(red, 10), (group, 20), (blue, 21), (green, 30)]:
        set_id(layer, layer_id)
    path = tmp_path / "skin.psd"
    psd.save(path)
    return path


def index_by_name(tree):
    return {layer["name"]: layer["index"] for layer in tree["layers"]}


def pixel(header, data, x, y):
    i = (y * header["width"] + x) * 4
    return tuple(data[i : i + 4])


def test_open_returns_tree(helper, psd_path):
    tree, _ = helper.send({"cmd": "open", "path": str(psd_path)})
    assert tree["ok"] and (tree["width"], tree["height"]) == (8, 8)
    assert tree["depth"] == 8 and tree["color_mode"] == "RGB"
    by_name = {layer["name"]: layer for layer in tree["layers"]}
    assert set(by_name) == {"Red", "G", "Blue/Bleu", "Green"}
    assert by_name["Blue/Bleu"]["path"] == ["G", "Blue/Bleu"]
    assert by_name["Blue/Bleu"]["parent"] == by_name["G"]["index"]
    assert by_name["Red"]["parent"] is None
    assert by_name["G"]["kind"] == "group"
    assert by_name["Green"]["visible"] is False
    assert [by_name[n]["id"] for n in ("Red", "G", "Blue/Bleu", "Green")] == [10, 20, 21, 30]


@pytest.mark.parametrize(
    "names, top_left, bottom_right",
    [
        (["Red"], RED, RED),
        (["Red", "G", "Blue/Bleu"], BLUE, RED),
        (["Red", "Blue/Bleu"], RED, RED),  # group not listed → its children are not drawn
        (["Red", "G", "Blue/Bleu", "Green"], GREEN, GREEN),  # hidden in PSD, forced on
        ([], EMPTY, EMPTY),
    ],
)
def test_render_follows_visible_list(helper, psd_path, names, top_left, bottom_right):
    tree, _ = helper.send({"cmd": "open", "path": str(psd_path)})
    idx = index_by_name(tree)
    header, data = helper.send({"cmd": "render", "visible": [idx[n] for n in names]})
    assert header["ok"] and len(data) == 8 * 8 * 4
    assert pixel(header, data, 0, 0)[3] == top_left[3]
    if top_left[3]:
        assert pixel(header, data, 0, 0) == top_left
    if bottom_right[3]:
        assert pixel(header, data, 7, 7) == bottom_right


def test_render_scale(helper, psd_path):
    helper.send({"cmd": "open", "path": str(psd_path)})
    header, data = helper.send({"cmd": "render", "visible": [0], "scale": 0.5})
    assert (header["width"], header["height"]) == (4, 4) and len(data) == 64


def test_blend_mode_is_applied(helper, tmp_path):
    psd = PSDImage.new("RGB", (4, 4))
    PixelLayer.frompil(Image.new("RGBA", (4, 4), (200, 100, 50, 255)), psd, "Base")
    top = PixelLayer.frompil(Image.new("RGBA", (4, 4), (128, 128, 128, 255)), psd, "Mul")
    top.blend_mode = BlendMode.MULTIPLY
    path = tmp_path / "blend.psd"
    psd.save(path)
    helper.send({"cmd": "open", "path": str(path)})
    header, data = helper.send({"cmd": "render", "visible": [0, 1]})
    r, g, b, a = pixel(header, data, 0, 0)
    assert max(abs(r - 100), abs(g - 50), abs(b - 25)) <= 2 and a == 255


def test_errors_keep_process_alive(helper, psd_path, tmp_path):
    not_psd = tmp_path / "notes.psd"
    not_psd.write_text("hello")
    cases = [
        ({"cmd": "render", "visible": []}, "protocol"),
        ({"cmd": "open", "path": str(not_psd)}, "invalid_psd"),
        ({"cmd": "open", "path": str(tmp_path / "missing.psd")}, "not_found"),
        ("{not json", "protocol"),
        ({"cmd": "dance"}, "protocol"),
    ]
    for req, kind in cases:
        header, _ = helper.send(req)
        assert header == {"ok": False, "kind": kind, "error": header["error"]}
    helper.send({"cmd": "open", "path": str(psd_path)})
    for bad in ([99], [-1], ["0"], None):
        header, _ = helper.send({"cmd": "render", "visible": bad})
        assert header["kind"] == "protocol"
    header, _ = helper.send({"cmd": "render", "visible": [0], "scale": 2})
    assert header["kind"] == "protocol"
    header, data = helper.send({"cmd": "render", "visible": [0]})
    assert header["ok"] and len(data) == 256


def test_stray_output_does_not_corrupt_protocol(psd_path):
    # Simulates a dependency that prints (Python level) and writes to fd 1 (C level).
    noisy = (
        "import os\n"
        "from psd_tools import PSDImage\n"
        "_composite = PSDImage.composite\n"
        "def _noisy(*a, **k):\n"
        "    print('parasite')\n"
        "    os.write(1, b'parasite-fd1\\n')\n"
        "    return _composite(*a, **k)\n"
        "PSDImage.composite = _noisy\n"
    )
    h = Helper(prefix=noisy)
    try:
        h.send({"cmd": "open", "path": str(psd_path)})
        header, data = h.send({"cmd": "render", "visible": [0]})
        assert header["ok"] and pixel(header, data, 3, 3) == RED
        header, _ = h.send({"cmd": "render", "visible": [0]})
        assert header["ok"]
    finally:
        h.close()


def test_saved_returns_photoshop_composite(helper, psd_path):
    helper.send({"cmd": "open", "path": str(psd_path)})
    header, data = helper.send({"cmd": "saved"})
    assert header["ok"] and (header["width"], header["height"]) == (8, 8)
    assert pixel(header, data, 0, 0) == BLUE and pixel(header, data, 7, 7) == RED


def test_saved_without_composite_says_so(helper, tmp_path):
    psd = PSDImage.new("RGB", (4, 4))
    PixelLayer.frompil(Image.new("RGBA", (4, 4), RED), psd, "Red")
    # What Photoshop writes when "Maximize compatibility" is off.
    psd.image_resources[Resource.VERSION_INFO] = ImageResource(
        key=Resource.VERSION_INFO, data=VersionInfo(has_composite=False)
    )
    path = tmp_path / "nocomp.psd"
    psd.save(path)
    helper.send({"cmd": "open", "path": str(path)})
    header, _ = helper.send({"cmd": "saved"})
    assert header["ok"] is False and header["kind"] == "no_composite"
