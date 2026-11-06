import math
import os
import subprocess
from PIL import Image, ImageDraw, ImageFilter

def create_mac_app_icon():
    size = 1024
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Full-bleed macOS squircle geometry (pad=0 so macOS never creates an outer white legacy frame!)
    # Apple standard corner radius for 1024x1024 icons is ~224px
    radius = 224
    icon_rect = [0, 0, size, size]

    # 1. Background Canvas: Deep Studio Charcoal Brushed Gradient
    bg_canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg_draw = ImageDraw.Draw(bg_canvas)
    for y in range(size):
        frac = y / size
        # Subtle top-to-bottom studio lighting gradient
        r = int(36 - frac * 14)
        g = int(38 - frac * 14)
        b = int(46 - frac * 16)
        bg_draw.line([(0, y), (size, y)], fill=(r, g, b, 255))

    # Mask to full squircle
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(icon_rect, radius=radius, fill=255)
    img.paste(bg_canvas, (0, 0), mask)

    draw = ImageDraw.Draw(img)

    # 2. Subtle Outer Bezel / Border (Tactile studio edge)
    draw.rounded_rectangle(icon_rect, radius=radius, outline=(65, 70, 85, 255), width=4)
    draw.rounded_rectangle([2, 2, size - 2, size - 2], radius=radius - 2, outline=(20, 22, 28, 220), width=3)

    # 3. Draw Studio Cassette Tape in Center
    cx, cy = size // 2, size // 2 - 30
    cass_w = 660
    cass_h = 420
    cass_left = cx - cass_w // 2
    cass_top = cy - cass_h // 2
    cass_right = cass_left + cass_w
    cass_bot = cass_top + cass_h

    # Cassette Shell Drop Shadow
    cass_shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    cs_draw = ImageDraw.Draw(cass_shadow)
    cs_draw.rounded_rectangle([cass_left - 6, cass_top + 16, cass_right + 6, cass_bot + 22], radius=28, fill=(0, 0, 0, 160))
    cass_shadow = cass_shadow.filter(ImageFilter.GaussianBlur(18))
    img.paste(cass_shadow, (0, 0), cass_shadow)

    # Cassette Shell (Matte Anthracite / Vintage Dark Finish)
    draw.rounded_rectangle([cass_left, cass_top, cass_right, cass_bot], radius=28, fill=(24, 25, 30, 255), outline=(60, 64, 76, 255), width=5)

    # Cassette Screws
    screw_r = 7
    screws = [
        (cass_left + 26, cass_top + 26),
        (cass_right - 26, cass_top + 26),
        (cass_left + 26, cass_bot - 26),
        (cass_right - 26, cass_bot - 26),
        (cx, cass_bot - 26)
    ]
    for sx, sy in screws:
        draw.ellipse([sx - screw_r, sy - screw_r, sx + screw_r, sy + screw_r], fill=(70, 75, 88, 255), outline=(18, 20, 24, 255), width=2)
        draw.line([sx - 4, sy - 4, sx + 4, sy + 4], fill=(28, 30, 36, 255), width=2)

    # Label Area (Warm Vintage Gold / Amber Band)
    lbl_pad_x = 52
    lbl_pad_y = 42
    lbl_rect = [cass_left + lbl_pad_x, cass_top + lbl_pad_y, cass_right - lbl_pad_x, cass_bot - lbl_pad_y - 48]
    draw.rounded_rectangle(lbl_rect, radius=16, fill=(218, 172, 85, 255), outline=(155, 120, 50, 255), width=4)

    # Label Decorative Strip (Studio Burgundy & Midnight Blue)
    strip_y = lbl_rect[1] + 38
    draw.rectangle([lbl_rect[0] + 8, strip_y, lbl_rect[2] - 8, strip_y + 8], fill=(195, 42, 42, 255))
    draw.rectangle([lbl_rect[0] + 8, strip_y + 11, lbl_rect[2] - 8, strip_y + 16], fill=(30, 50, 90, 255))

    # Center Transparent Tape Window
    win_w = 310
    win_h = 150
    win_left = cx - win_w // 2
    win_top = cy - win_h // 2 - 5
    win_right = win_left + win_w
    win_bot = win_top + win_h
    draw.rounded_rectangle([win_left, win_top, win_right, win_bot], radius=14, fill=(16, 17, 21, 255), outline=(75, 80, 95, 255), width=3)

    # Spools Left & Right (White / Metallic Toothed Hubs)
    spool_r = 52
    spool_lx = cx - 88
    spool_rx = cx + 88
    spool_y = cy - 5

    # Brown Magnetic Tape Packs behind spools
    draw.ellipse([spool_lx - 72, spool_y - 72, spool_lx + 72, spool_y + 72], fill=(68, 38, 22, 255))
    draw.ellipse([spool_rx - 62, spool_y - 62, spool_rx + 62, spool_y + 62], fill=(68, 38, 22, 255))

    # White Toothed Hubs
    for sp_x in [spool_lx, spool_rx]:
        draw.ellipse([sp_x - spool_r, spool_y - spool_r, sp_x + spool_r, spool_y + spool_r], fill=(242, 244, 248, 255), outline=(175, 180, 192, 255), width=3)
        # Center hole
        hole_r = 18
        draw.ellipse([sp_x - hole_r, spool_y - hole_r, sp_x + hole_r, spool_y + hole_r], fill=(16, 17, 21, 255))
        # 6 Teeth
        for i in range(6):
            ang = i * (math.pi / 3)
            tx = sp_x + math.cos(ang) * 26
            ty = spool_y + math.sin(ang) * 26
            draw.ellipse([tx - 5, ty - 5, tx + 5, ty + 5], fill=(175, 180, 192, 255))

    # Tape Guide Roller Trapezoid at bottom of cassette
    trap_pts = [
        (cx - 165, cass_bot),
        (cx + 165, cass_bot),
        (cx + 130, cass_bot - 52),
        (cx - 130, cass_bot - 52),
    ]
    draw.polygon(trap_pts, fill=(32, 34, 40, 255), outline=(60, 64, 76, 255))
    # Two bronze rollers inside trapezoid
    draw.ellipse([cx - 105 - 11, cass_bot - 28 - 11, cx - 105 + 11, cass_bot - 28 + 11], fill=(195, 145, 65, 255))
    draw.ellipse([cx + 105 - 11, cass_bot - 28 - 11, cx + 105 + 11, cass_bot - 28 + 11], fill=(195, 145, 65, 255))

    # 4. Glowing Studio Waveform below Cassette
    wave_y = size - 140
    wave_bars = [16, 32, 50, 74, 98, 120, 92, 114, 128, 95, 70, 105, 122, 80, 56, 36, 20]
    n_bars = len(wave_bars)
    bar_span = 560
    step = bar_span / (n_bars - 1)
    bx_start = cx - bar_span // 2

    for i, h in enumerate(wave_bars):
        bx = bx_start + i * step
        # Cyan to violet gradient
        frac = i / (n_bars - 1)
        r = int(56 + frac * 90)
        g = int(189 - frac * 50)
        b = int(248)
        draw.line([(bx, wave_y - h // 2), (bx, wave_y + h // 2)], fill=(r, g, b, 240), width=8)

    # 5. Export Assets
    os.makedirs("assets", exist_ok=True)
    img.save("assets/AppIcon.png")

    # Export 128x128 raw RGBA for Rust binary static embedding
    icon_128 = img.resize((128, 128), Image.Resampling.LANCZOS)
    with open("assets/icon_128.rgba", "wb") as f:
        f.write(icon_128.tobytes())

    # 6. Export macOS .iconset for iconutil
    iconset_dir = "TapeDigizer.iconset"
    os.makedirs(iconset_dir, exist_ok=True)

    sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]

    for s, name in sizes:
        resized = img.resize((s, s), Image.Resampling.LANCZOS)
        resized.save(os.path.join(iconset_dir, name))

    # Run iconutil to create AppIcon.icns
    subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", "AppIcon.icns"], check=True)
    print("Successfully created AppIcon.icns and assets/icon_128.rgba!")

if __name__ == "__main__":
    create_mac_app_icon()
