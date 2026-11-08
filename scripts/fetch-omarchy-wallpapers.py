#!/usr/bin/env python3
"""
Fetches theme wallpapers from omacom/omarchy repo, converts to high-quality JPG,
and places them into uis/sway/files/home/Pictures/Wallpapers/ and host ~/Pictures/Wallpapers/.
"""

import os
import subprocess
import urllib.request

WALLPAPERS = {
    "nord": [
        ("0-black-moon.jpg", "nord-black-moon.jpg"),
        ("1-city-view.webp", "nord-city-view.jpg"),
        ("2-night-hawks.webp", "nord-night-hawks.jpg"),
    ],
    "tokyo-night": [
        ("6-oma-cityscape.jpg", "tokyo-night-cityscape.jpg"),
        ("1-winding-road.webp", "tokyo-night-winding-road.jpg"),
        ("4-sunset-lake.webp", "tokyo-night-sunset-lake.jpg"),
        ("0-liftoff.webp", "tokyo-night-liftoff.jpg"),
    ],
    "catppuccin": [
        ("1-totoro.webp", "catppuccin-totoro.jpg"),
        ("2-waves.webp", "catppuccin-waves.jpg"),
        ("3-blue-eye.webp", "catppuccin-blue-eye.jpg"),
    ],
    "rose-pine": [
        ("3-omarchy-plants.webp", "rose-pine-plants.jpg"),
        ("1-funky-shapes.webp", "rose-pine-funky-shapes.jpg"),
        ("2-dot-map.webp", "rose-pine-dot-map.jpg"),
    ],
    "gruvbox": [
        ("1-the-backwater.jpg", "gruvbox-the-backwater.jpg"),
        ("2-flower-basket.webp", "gruvbox-flower-basket.jpg"),
        ("3-village-square.jpg", "gruvbox-village-square.jpg"),
        ("5-leaves.jpg", "gruvbox-leaves.jpg"),
    ],
    "everforest": [
        ("1-tree-tops.webp", "everforest-tree-tops.jpg"),
    ],
    "kanagawa": [
        ("1-kanagawa.jpg", "kanagawa-wave.jpg"),
    ],
}

BASE_URL = "https://raw.githubusercontent.com/omacom/omarchy/quattro/themes"
DEST_REPO = os.path.abspath("uis/sway/files/home/Pictures/Wallpapers")
DEST_HOST = os.path.expanduser("~/Pictures/Wallpapers")

os.makedirs(DEST_REPO, exist_ok=True)
os.makedirs(DEST_HOST, exist_ok=True)

for theme, files in WALLPAPERS.items():
    for src_name, dest_name in files:
        url = f"{BASE_URL}/{theme}/backgrounds/{src_name}"
        tmp_path = f"/tmp/{src_name}"
        repo_dest = os.path.join(DEST_REPO, dest_name)
        host_dest = os.path.join(DEST_HOST, dest_name)

        print(f"Downloading {theme}/{src_name} -> {dest_name}...")
        try:
            urllib.request.urlretrieve(url, tmp_path)
            # Convert to high-quality JPEG (scale to max 2560x1440 for performance and sharpness)
            cmd = [
                "ffmpeg", "-y", "-i", tmp_path,
                "-vf", "scale='min(2560,iw)':'min(1440,ih)':force_original_aspect_ratio=decrease",
                "-q:v", "2",
                repo_dest
            ]
            subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            # Copy to host as well
            subprocess.run(["cp", "-f", repo_dest, host_dest], check=True)
            print(f"  Saved {repo_dest} ({os.path.getsize(repo_dest) // 1024} KB)")
        except Exception as e:
            print(f"  Error downloading {url}: {e}")
        finally:
            if os.path.exists(tmp_path):
                os.remove(tmp_path)

print("Done fetching wallpapers!")
