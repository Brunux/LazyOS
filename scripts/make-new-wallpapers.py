#!/usr/bin/env python3
"""
Generates high-resolution 2560x1440 aesthetic wallpapers for the ported Omarchy themes.
Renders to raw PPM and encodes to crisp JPEG using ffmpeg.
"""

import math
import os
import subprocess

W = 2560
H = 1440

DEST_DIRS = [
    os.path.abspath("uis/sway/files/home/Pictures/Wallpapers"),
    os.path.expanduser("~/Pictures/Wallpapers"),
]

for d in DEST_DIRS:
    os.makedirs(d, exist_ok=True)

def clamp(v):
    return max(0, min(255, int(v)))

def hex_to_rgb(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i+2], 16) for i in (0, 2, 4))

def render_wallpaper(filename, generator_fn):
    print(f"Generating {filename} ({W}x{H})...")
    ppm_path = f"/tmp/{filename}.ppm"
    
    with open(ppm_path, "wb") as f:
        f.write(f"P6\n{W} {H}\n255\n".encode("ascii"))
        for y in range(H):
            row = bytearray()
            for x in range(W):
                r, g, b = generator_fn(x, y, W, H)
                row.extend([clamp(r), clamp(g), clamp(b)])
            f.write(row)
            
    for d in DEST_DIRS:
        out_jpg = os.path.join(d, filename)
        cmd = [
            "ffmpeg", "-y", "-i", ppm_path,
            "-q:v", "2",
            out_jpg
        ]
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print(f"  Saved {out_jpg} ({os.path.getsize(out_jpg) // 1024} KB)")
        
    os.remove(ppm_path)

# 1. Dracula: Cosmic violet dark atmosphere with soft dual magenta/cyan glow
def gen_dracula(x, y, w, h):
    nx, ny = x / w, y / h
    # Base dark violet-gray
    r, g, b = 40, 42, 54 # #282a36
    # Radial glow from center-right (cyan) and center-left (pink/purple)
    d1 = math.hypot(nx - 0.75, ny - 0.45)
    d2 = math.hypot(nx - 0.25, ny - 0.55)
    glow_cyan = math.exp(-d1 * 2.2) * 0.45
    glow_pink = math.exp(-d2 * 2.5) * 0.55
    r += glow_pink * 140
    g += glow_pink * 50 + glow_cyan * 90
    b += glow_pink * 160 + glow_cyan * 120
    # Vignette
    vig = 1.0 - 0.35 * (math.hypot(nx - 0.5, ny - 0.5) ** 1.8)
    return r * vig, g * vig, b * vig

# 2. Catppuccin Latte: Soft, soothing warm day pastel
def gen_latte(x, y, w, h):
    nx, ny = x / w, y / h
    # Gradient from subtle lavender-tinted white (#eff1f5) to soft warm mantle (#e6e9ef)
    r = 239 - ny * 12 + math.sin(nx * 3.14) * 4
    g = 241 - ny * 10 + math.sin(nx * 3.14) * 3
    b = 245 - ny * 8
    # Subtle soft blue/rosewater accent warmth in upper-center
    d = math.hypot(nx - 0.5, ny - 0.3)
    glow = math.exp(-d * 3.0) * 0.12
    r += glow * 20
    g += glow * 10
    b += glow * 35
    return r, g, b

# 3. Hackerman: High-tech terminal pitch black with matrix phosphor glow & subtle grid
def gen_hackerman(x, y, w, h):
    nx, ny = x / w, y / h
    # #0b0c16
    r, g, b = 11, 12, 22
    # Cyber glow from bottom center
    d = math.hypot(nx - 0.5, ny - 0.9)
    glow = math.exp(-d * 2.0) * 0.7
    r += glow * 20
    g += glow * 220
    b += glow * 110
    # Subtle cyber grid
    grid_x = (x % 64 == 0)
    grid_y = (y % 64 == 0)
    if grid_x or grid_y:
        intensity = 18 if (grid_x and grid_y) else 9
        g += intensity
        b += intensity // 2
    return r, g, b

# 4. Lumon: Cold corporate teal void with sleek horizon glow
def gen_lumon(x, y, w, h):
    nx, ny = x / w, y / h
    # #16242d deep navy
    r, g, b = 22, 36, 45
    # Horizon light line at y=0.62
    dist_horiz = abs(ny - 0.62)
    horiz_glow = math.exp(-dist_horiz * 14.0) * 0.45
    # Center focus
    rad = math.hypot(nx - 0.5, ny - 0.6)
    center_glow = math.exp(-rad * 2.5) * 0.3
    teal_boost = horiz_glow + center_glow
    r += teal_boost * 70
    g += teal_boost * 140
    b += teal_boost * 185
    # Vignette
    vig = 1.0 - 0.3 * (math.hypot(nx - 0.5, ny - 0.5) ** 1.6)
    return r * vig, g * vig, b * vig

# 5. Matte Black: Minimalist deep-black OLED gradient with satin sheen
def gen_matte_black(x, y, w, h):
    nx, ny = x / w, y / h
    # Subtle charcoal curve from #0e0e10 to #1a1a20
    dist = math.hypot(nx - 0.5, ny - 0.4)
    sheen = math.exp(-dist * 1.8) * 22.0
    val = 14 + sheen
    return val, val, val * 1.08

# 6. Omacon: Deep cosmic purple with neon magenta halo
def gen_omacon(x, y, w, h):
    nx, ny = x / w, y / h
    # #130c2b
    r, g, b = 19, 12, 43
    # Intense electric magenta pulse
    d = math.hypot(nx - 0.5, ny - 0.5)
    pulse = math.exp(-d * 2.2) * 0.65
    r += pulse * 210
    g += pulse * 20
    b += pulse * 210
    # Secondary violet halo
    d2 = math.hypot(nx - 0.7, ny - 0.3)
    pulse2 = math.exp(-d2 * 3.0) * 0.35
    r += pulse2 * 140
    b += pulse2 * 190
    vig = 1.0 - 0.4 * (d ** 1.5)
    return r * vig, g * vig, b * vig

# 7. Flexoki Light: Crisp warm editorial paper with subtle ambient gradient
def gen_flexoki_light(x, y, w, h):
    nx, ny = x / w, y / h
    # #fffcf0 (255, 252, 240) to #f2f0e5 (242, 240, 229)
    r = 255 - ny * 13
    g = 252 - ny * 12
    b = 240 - ny * 11
    # Subtle warm paper tone variation
    warmth = math.sin(nx * 3.1415) * math.cos(ny * 3.1415) * 3.0
    return r + warmth, g + warmth, b + warmth * 0.8

# 8. One Dark: Sleek modern developer slate with subtle indigo core
def gen_one_dark(x, y, w, h):
    nx, ny = x / w, y / h
    # #282c34 (40, 44, 52)
    r, g, b = 40, 44, 52
    d = math.hypot(nx - 0.5, ny - 0.45)
    core = math.exp(-d * 2.0) * 0.35
    r += core * 35
    g += core * 65
    b += core * 110
    vig = 1.0 - 0.3 * (d ** 1.8)
    return r * vig, g * vig, b * vig

WALLPAPERS = [
    ("dracula-night.jpg", gen_dracula),
    ("catppuccin-latte-meadow.jpg", gen_latte),
    ("hackerman-matrix.jpg", gen_hackerman),
    ("lumon-severance.jpg", gen_lumon),
    ("matte-black-minimal.jpg", gen_matte_black),
    ("omacon-cosmic.jpg", gen_omacon),
    ("flexoki-light-paper.jpg", gen_flexoki_light),
    ("one-dark-space.jpg", gen_one_dark),
]

for name, gen in WALLPAPERS:
    render_wallpaper(name, gen)

print("All wallpapers generated successfully!")
