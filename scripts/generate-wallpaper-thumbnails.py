#!/usr/bin/env python3
"""
Generates 36x12 and 48x16 PNG thumbnails for all wallpapers in uis/sway/files/home/Pictures/Wallpapers/
and copies them to ~/.cache/lazyos/wallpapers/thumbnails/ and ~/Pictures/Wallpapers/thumbnails/.
"""

import os
import subprocess
import glob

WP_DIRS = [
    os.path.abspath("uis/sway/files/home/Pictures/Wallpapers"),
    os.path.expanduser("~/Pictures/Wallpapers"),
]

for wp_dir in WP_DIRS:
    if not os.path.exists(wp_dir):
        continue
    thumb_dir = os.path.join(wp_dir, "thumbnails")
    os.makedirs(thumb_dir, exist_ok=True)
    
    # Process all jpg and png files (except inside thumbnails)
    for ext in ("*.jpg", "*.png"):
        for path in glob.glob(os.path.join(wp_dir, ext)):
            filename = os.path.basename(path)
            thumb_path = os.path.join(thumb_dir, f"{filename}.png")
            
            # Generate 40x16 thumbnail with center crop
            cmd = [
                "ffmpeg", "-y", "-i", path,
                "-vf", "scale=40:16:force_original_aspect_ratio=increase,crop=40:16",
                thumb_path
            ]
            try:
                subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                print(f"Generated {thumb_path}")
                # Also generate clean name without duplicate ext
                clean_name = os.path.splitext(filename)[0] + ".png"
                clean_path = os.path.join(thumb_dir, clean_name)
                if clean_path != thumb_path:
                    subprocess.run(["cp", "-f", thumb_path, clean_path])
            except Exception as e:
                print(f"Error generating thumbnail for {path}: {e}")

# Also copy to cache directory
cache_thumb_dir = os.path.expanduser("~/.cache/lazyos/wallpapers/thumbnails")
os.makedirs(cache_thumb_dir, exist_ok=True)
for path in glob.glob(os.path.join(WP_DIRS[0], "thumbnails", "*.png")):
    filename = os.path.basename(path)
    subprocess.run(["cp", "-f", path, os.path.join(cache_thumb_dir, filename)])

print("Thumbnails generated successfully!")
