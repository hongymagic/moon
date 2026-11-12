#!/usr/bin/env python3
"""Regenerate moon's original vector mark and Windows icons; Python standard library only."""
from pathlib import Path
import math
import struct
import zlib

ROOT = Path(__file__).resolve().parent
SIZES = (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256)
COLORS = {"moon": None, "moon-tray-light": (37, 34, 37), "moon-tray-dark": (240, 235, 224)}


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))


def png(size, pixels):
    rows = b''.join(b'\0' + pixels[y * size * 4:(y + 1) * size * 4] for y in range(size))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b'')


def render(size, color=None):
    # Optical small-size master: a slightly fuller crescent keeps its tips legible.
    inner_radius = 18.3 if size <= 24 else 19.0
    samples = 6 if size < 64 else 4
    pixels = bytearray()
    for py in range(size):
        for px in range(size):
            coverage = 0
            for sy in range(samples):
                y = (py + (sy + .5) / samples) * 48 / size
                for sx in range(samples):
                    x = (px + (sx + .5) / samples) * 48 / size
                    if (x - 24) ** 2 + (y - 24) ** 2 <= 20 ** 2 and (x - 31) ** 2 + (y - 17) ** 2 >= inner_radius ** 2:
                        coverage += 1
            if color is None:
                t = min(1, max(0, (px + py + 1) / (2 * size)))
                stops = ((239, 192, 139), (209, 122, 79), (153, 73, 46))
                a, b = (stops[0], stops[1]) if t <= .55 else (stops[1], stops[2])
                f = t / .55 if t <= .55 else (t - .55) / .45
                rgb = tuple(round(a[i] + (b[i] - a[i]) * f) for i in range(3))
            else:
                rgb = color
            pixels.extend((*rgb, round(255 * coverage / samples ** 2)))
    return bytes(pixels)


def dib(size, pixels):
    stride = ((size + 31) // 32) * 4
    mask = bytearray(stride * size)
    bitmap = bytearray()
    for row in range(size - 1, -1, -1):
        for x in range(size):
            r, g, b, a = pixels[(row * size + x) * 4:(row * size + x + 1) * 4]
            bitmap.extend((b, g, r, a))
            if a == 0:
                mask[(size - row - 1) * stride + x // 8] |= 0x80 >> (x % 8)
    return struct.pack('<IiiHHIIiiII', 40, size, size * 2, 1, 32, 0, len(bitmap), 0, 0, 0, 0) + bitmap + mask


def ico(images):
    payloads = [(size, png(size, data) if size == 256 else dib(size, data)) for size, data in images]
    offset = 6 + len(payloads) * 16
    directory = bytearray(struct.pack('<HHH', 0, 1, len(payloads)))
    for size, data in payloads:
        directory.extend(struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(data), offset))
        offset += len(data)
    return bytes(directory) + b''.join(data for _, data in payloads)


def svg(color=None):
    fill = color or 'url(#light)'
    gradient = '' if color else '<linearGradient id="light" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#efc08b"/><stop offset=".55" stop-color="#d17a4f"/><stop offset="1" stop-color="#99492e"/></linearGradient>'
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none"><title>moon</title><defs>{gradient}<mask id="crescent"><circle cx="24" cy="24" r="20" fill="white"/><circle cx="31" cy="17" r="19" fill="black"/></mask></defs><circle cx="24" cy="24" r="20" fill="{fill}" mask="url(#crescent)"/></svg>\n'''


def main():
    for name, color in COLORS.items():
        images = [(size, render(size, color)) for size in SIZES]
        (ROOT / f'{name}.ico').write_bytes(ico(images))
        (ROOT / f'{name}.svg').write_text(svg(None if color is None else '#%02x%02x%02x' % color))
        for size, pixels in images:
            if name == 'moon' or size in (16, 20, 24, 32, 48, 64):
                (ROOT / f'{name}-{size}.png').write_bytes(png(size, pixels))
    print('Generated three original SVG/ICO families at', ', '.join(map(str, SIZES)), 'pixels.')


if __name__ == '__main__':
    main()
