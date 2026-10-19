"""Times the helper on a PSD and compares its render with the composite Photoshop saved.

  python helper/bench_psd_helper.py skin.psd [more.psd …]
  python helper/bench_psd_helper.py --synthetic   # 4096×4096, 50 layers, generated

The comparison shows whether psd-tools draws this PSD like Photoshop (layer effects,
adjustment layers): a large mean difference means the preview will not match.
"""

import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import Group, PixelLayer

SCRIPT = (Path(__file__).parent / "psd_helper.py").read_text()


def synthetic(path, size=4096, layers=50):
    rng = np.random.default_rng(0)
    psd = PSDImage.new("RGB", (size, size))
    PixelLayer.frompil(Image.new("RGBA", (size, size), (40, 60, 160, 255)), psd, "Base")
    parent = psd
    for i in range(layers - 1):
        if i % 10 == 0:
            parent = Group.new(psd, f"Groupe {i // 10}")
        w, h = rng.integers(size // 16, size // 2, 2)
        pixels = rng.integers(0, 255, (h, w, 4), dtype=np.uint8)
        pixels[..., 3] = 255
        left, top = rng.integers(0, size - max(w, h), 2)
        PixelLayer.frompil(Image.fromarray(pixels, "RGBA"), parent, f"Sticker {i}", top=int(top), left=int(left))
    psd.save(path)


def send(proc, req):
    proc.stdin.write(json.dumps(req).encode() + b"\n")
    proc.stdin.flush()
    header = json.loads(proc.stdout.readline())
    data = proc.stdout.read(header["bytes"]) if header.get("bytes") else None
    return header, data


def bench(path):
    proc = subprocess.Popen([sys.executable, "-I", "-c", SCRIPT], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    t = time.perf_counter()
    tree, _ = send(proc, {"cmd": "open", "path": str(path)})
    t_open = time.perf_counter() - t
    if not tree["ok"]:
        print(f"{path}: {tree}")
        return
    visible = visible_as_in_psd(tree["layers"])
    timings = {}
    for scale in (1.0, 0.25):
        t = time.perf_counter()
        header, data = send(proc, {"cmd": "render", "visible": visible, "scale": scale})
        timings[scale] = time.perf_counter() - t
        if scale == 1.0:
            full = np.frombuffer(data, np.uint8).reshape(header["height"], header["width"], 4)
    send(proc, {"cmd": "quit"})
    proc.wait()

    n_effects = sum(layer["has_effects"] for layer in tree["layers"])
    kinds = sorted({layer["kind"] for layer in tree["layers"]})
    print(f"{path.name}: {tree['width']}×{tree['height']} {tree['depth']} bits {tree['color_mode']}, "
          f"{len(tree['layers'])} calques ({', '.join(kinds)}), {n_effects} avec effets")
    print(f"  open {t_open:.2f} s · render 100 % {timings[1.0]:.2f} s · render 25 % {timings[0.25]:.2f} s")
    print(f"  ids absents : {sum(layer['id'] is None for layer in tree['layers'])}")

    saved = PSDImage.open(path)
    if saved.has_preview():
        preview = np.asarray(saved.topil().convert("RGBA"), dtype=np.int16)
        diff = np.abs(preview[..., :3] - full[..., :3].astype(np.int16))
        print(f"  écart avec le composite Photoshop : moyen {diff.mean():.2f}, "
              f"pixels > 16 : {(diff.max(axis=2) > 16).mean() * 100:.2f} %")
    else:
        print("  pas de composite enregistré dans le PSD (comparaison impossible)")


def visible_as_in_psd(layers):
    return [layer["index"] for layer in layers if layer["visible"]]


def main(args):
    if args == ["--synthetic"]:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "synthetic.psd"
            t = time.perf_counter()
            synthetic(path)
            print(f"PSD synthétique généré en {time.perf_counter() - t:.1f} s")
            bench(path)
        return
    if not args:
        sys.exit(__doc__)
    for arg in args:
        bench(Path(arg))


if __name__ == "__main__":
    main(sys.argv[1:])
