import math
import os
import subprocess
from PIL import Image, ImageDraw, ImageFilter

def create_mac_app_icon():
    size = 1024
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. macOS Squircle / Rounded Rect Shadow
    # macOS standard icon size is ~824x824 centered in 1024x1024 with drop shadow
    pad = 96
    icon_rect = [pad, pad, size - pad, size - pad]
    radius = 185

    # Shadow
    shadow_img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow_img)
    shadow_draw.rounded_rectangle([pad + 4, pad + 24, size - pad + 4, size - pad + 32], radius=radius, fill=(0, 0, 0, 110))
    shadow_img = shadow_img.filter(ImageFilter.GaussianBlur(24))
    img.paste(shadow_img, (0, 0), shadow_img)

    # 2. Main Icon Background: Deep Studio Charcoal with subtle brushed gradient
    # We draw subtle gradient
    bg_canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    bg_draw = ImageDraw.Draw(bg_canvas)
    for y in range(pad, size - pad):
        frac = (y - pad) / (size - 2 * pad)
        r = int(38 + frac * 8)
        g = int(40 + frac * 8)
        b = int(46 + frac * 10)
        bg_draw.line([(pad, y), (size - pad, y)], fill=(r, g, b, 255))

    # Mask to rounded rect
    mask = Image.new("L", (size, size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(icon_rect, radius=radius, fill=255)
    img.paste(bg_canvas, (0, 0), mask)

    # 3. Outer Metallic Bezel
    draw.rounded_rectangle(icon_rect, radius=radius, outline=(75, 80, 95, 255), width=3)
    draw.rounded_rectangle([pad + 2, pad + 2, size - pad - 2, size - pad - 2], radius=radius - 2, outline=(25, 26, 30, 200), width=2)

    # 4. Draw Studio Cassette Tape in Center
    cx, cy = size // 2, size // 2
    cass_w = 560
    cass_h = 360
    cass_left = cx - cass_w // 2
    cass_top = cy - cass_h // 2 - 20
    cass_right = cass_left + cass_w
    cass_bot = cass_top + cass_h

    # Cassette Shell Drop Shadow
    cass_shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    cs_draw = ImageDraw.Draw(cass_shadow)
    cs_draw.rounded_rectangle([cass_left - 4, cass_top + 12, cass_right + 4, cass_bot + 16], radius=24, fill=(0, 0, 0, 140))
    cass_shadow = cass_shadow.filter(ImageFilter.GaussianBlur(14))
    img.paste(cass_shadow, (0, 0), cass_shadow)

    # Cassette Shell (Matte Anthracite / Vintage Dark Finish)
    draw.rounded_rectangle([cass_left, cass_top, cass_right, cass_bot], radius=24, fill=(26, 27, 32, 255), outline=(55, 58, 68, 255), width=4)

    # Cassette Screws
    screw_r = 6
    screws = [
        (cass_left + 22, cass_top + 22),
        (cass_right - 22, cass_top + 22),
        (cass_left + 22, cass_bot - 22),
        (cass_right - 22, cass_bot - 22),
        (cx, cass_bot - 22)
    ]
    for sx, sy in screws:
        draw.ellipse([sx - screw_r, sy - screw_r, sx + screw_r, sy + screw_r], fill=(60, 64, 74, 255), outline=(20, 22, 25, 255), width=2)
        draw.line([sx - 4, sy - 4, sx + 4, sy + 4], fill=(30, 32, 38, 255), width=2)

    # Label Area (Warm Vintage Beige / Gold Band)
    lbl_pad_x = 44
    lbl_pad_y = 36
    lbl_rect = [cass_left + lbl_pad_x, cass_top + lbl_pad_y, cass_right - lbl_pad_x, cass_bot - lbl_pad_y - 40]
    draw.rounded_rectangle(lbl_rect, radius=14, fill=(215, 175, 95, 255), outline=(160, 125, 60, 255), width=3)

    # Label Decorative Strip (Studio Red & Navy Line)
    strip_y = lbl_rect[1] + 34
    draw.rectangle([lbl_rect[0] + 6, strip_y, lbl_rect[2] - 6, strip_y + 6], fill=(190, 45, 45, 255))
    draw.rectangle([lbl_rect[0] + 6, strip_y + 8, lbl_rect[2] - 6, strip_y + 12], fill=(35, 55, 95, 255))

    # Center Transparent Tape Window
    win_w = 260
    win_h = 130
    win_left = cx - win_w // 2
    win_top = cy - win_h // 2 - 20
    win_right = win_left + win_w
    win_bot = win_top + win_h
    draw.rounded_rectangle([win_left, win_top, win_right, win_bot], radius=12, fill=(18, 19, 23, 255), outline=(70, 75, 88, 255), width=3)

    # Spools Left & Right (White / Metallic Toothed Hubs)
    spool_r = 44
    spool_lx = cx - 74
    spool_rx = cx + 74
    spool_y = cy - 20

    # Brown Magnetic Tape Packs behind spools
    draw.ellipse([spool_lx - 62, spool_y - 62, spool_lx + 62, spool_y + 62], fill=(65, 38, 25, 255))
    draw.ellipse([spool_rx - 54, spool_y - 54, spool_rx + 54, spool_y + 54], fill=(65, 38, 25, 255))

    # White Hubs
    for sp_x in [spool_lx, spool_rx]:
        draw.ellipse([sp_x - spool_r, spool_y - spool_r, sp_x + spool_r, spool_y + spool_r], fill=(240, 242, 245, 255), outline=(180, 185, 195, 255), width=3)
        # Center hole
        hole_r = 16
        draw.ellipse([sp_x - hole_r, spool_y - hole_r, sp_x + hole_r, spool_y + hole_r], fill=(18, 19, 23, 255))
        # 6 Teeth
        for i in range(6):
            ang = i * (math.pi / 3)
            tx = sp_x + math.cos(ang) * 22
            ty = spool_y + math.sin(ang) * 22
            draw.ellipse([tx - 4, ty - 4, tx + 4, ty + 4], fill=(180, 185, 195, 255))

    # Tape Guide Roller Trapazoid at bottom of cassette
    trap_pts = [
        (cx - 140, cass_bot),
        (cx + 140, cass_bot),
        (cx + 110, cass_bot - 44),
        (cx - 110, cass_bot - 44),
    ]
    draw.polygon(trap_pts, fill=(34, 36, 42, 255), outline=(55, 58, 68, 255))
    # Two rollers inside trapazoid
    draw.ellipse([cx - 90 - 9, cass_bot - 24 - 9, cx - 90 + 9, cass_bot - 24 + 9], fill=(180, 140, 70, 255))
    draw.ellipse([cx + 90 - 9, cass_bot - 24 - 9, cx + 90 + 9, cass_bot - 24 + 9], fill=(180, 140, 70, 255))

    # 5. Glowing Studio Waveform & Digital Sound Rays below Cassette
    wave_y = size - pad - 90
    wave_bars = [12, 24, 38, 56, 74, 90, 68, 85, 96, 70, 52, 78, 92, 60, 42, 28, 16]
    n_bars = len(wave_bars)
    bar_span = 440
    step = bar_span / (n_bars - 1)
    bx_start = cx - bar_span // 2

    for i, h in enumerate(wave_bars):
        bx = bx_start + i * step
        # Cyan to violet gradient
        frac = i / (n_bars - 1)
        r = int(56 + frac * 90)
        g = int(189 - frac * 50)
        b = int(248)
        draw.line([(bx, wave_y - h // 2), (bx, wave_y + h // 2)], fill=(r, g, b, 240), width=6)

    # 6. Save Icon
    iconset_dir = "TapeDigizer.iconset"
    os.makedirs(iconset_dir, exist_ok=True)
    master_png = os.path.join(iconset_dir, "icon_512x512@2x.png")
    img.save(master_png)

    # Export all standard Apple iconset resolutions
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
    ]

    for s, name in sizes:
        resized = img.resize((s, s), Image.Resampling.LANCZOS)
        resized.save(os.path.join(iconset_dir, name))

    # Run iconutil to create AppIcon.icns
    subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", "AppIcon.icns"], check=True)
    print("Successfully created AppIcon.icns!")

if __name__ == "__main__":
    create_mac_app_icon()
