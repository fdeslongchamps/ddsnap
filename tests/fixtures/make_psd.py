"""Writes the integration-test PSD: python make_psd.py out.psd

64×64, bottom to top: Red (full), group G { Blue (top-left 32×32) }, Green (hidden).
Layer ids 10, 20, 21, 30.
"""
import sys

from PIL import Image
from psd_tools import PSDImage
from psd_tools.api.layers import Group, PixelLayer
from psd_tools.constants import Tag

psd = PSDImage.new("RGB", (64, 64))
red = PixelLayer.frompil(Image.new("RGBA", (64, 64), (255, 0, 0, 255)), psd, "Red")
group = Group.new(psd, "G")
blue = PixelLayer.frompil(Image.new("RGBA", (32, 32), (0, 0, 255, 255)), group, "Blue/Bleu")
green = PixelLayer.frompil(Image.new("RGBA", (64, 64), (0, 255, 0, 255)), psd, "Green")
green.visible = False
for layer, layer_id in [(red, 10), (group, 20), (blue, 21), (green, 30)]:
    layer._record.tagged_blocks.set_data(Tag.LAYER_ID, layer_id)
psd.save(sys.argv[1])
