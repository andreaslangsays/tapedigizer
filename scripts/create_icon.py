import math
import os
import subprocess
from PIL import Image, ImageDraw, ImageFilter

def create_mac_app_icon():
    # Apple macOS Human Interface Guidelines for App Icons (macOS 11 Big Sur to macOS 15 Sequoia+):
    # - Master Canvas: 1024 x 1024
    # - Squircle Base: 824 x 824 centered inside the canvas (100px margins on all 4 sides)
    # - Corner Radius: 185.4 px (~185 px)
    # - Drop Shadow: Black (0, 0, 0), opacity ~45-50%, y-offset +12px, blur 28px
    canvas_size = 1024
    margin = 100
    squircle_size = 824
    radius = 185
    sq_rect = [margin, margin, margin + squircle_size, margin + squircle_size] # [100, 100, 924, 924]

    master_img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))

    # --- 1. macOS System Drop Shadow ---
    shadow_img = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow_img)
    shadow_y_offset = 14
    shadow_draw.rounded_rectangle(
        [margin, margin + shadow_y_offset, margin + squircle_size, margin + squircle_size + shadow_y_offset],
        radius=radius,
        fill=(0, 0, 0, 115)
    )
    shadow_img = shadow_img.filter(ImageFilter.GaussianBlur(24))
    master_img.paste(shadow_img, (0, 0), shadow_img)

    # --- 2. Squircle Background Canvas (Deep Studio Charcoal) ---
    sq_canvas = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    sq_draw = ImageDraw.Draw(sq_canvas)
    for y in range(margin, margin + squircle_size):
        frac = (y - margin) / squircle_size
        r = int(36 - frac * 14)
        g = int(38 - frac * 14)
        b = int(46 - frac * 16)
        sq_draw.line([(margin, y), (margin + squircle_size, y)], fill=(r, g, b, 255))

    # Mask background into exact macOS squircle
    mask = Image.new("L", (canvas_size, canvas_size), 0)
    mask_draw = ImageDraw.Draw(mask)
    mask_draw.rounded_rectangle(sq_rect, radius=radius, fill=255)
    master_img.paste(sq_canvas, (0, 0), mask)

    draw = ImageDraw.Draw(master_img)

    # Tactile Studio Bezel & Edge Highlight
    draw.rounded_rectangle(sq_rect, radius=radius, outline=(70, 75, 90, 255), width=3)
    draw.rounded_rectangle(
        [sq_rect[0] + 2, sq_rect[1] + 2, sq_rect[2] - 2, sq_rect[3] - 2],
        radius=radius - 2,
        outline=(22, 24, 30, 200),
        width=2
    )

    # --- 3. Studio Cassette Tape in Center ---
    cx = 512
    cy = 470
    cass_w = 540
    cass_h = 345
    cass_left = cx - cass_w // 2
    cass_top = cy - cass_h // 2
    cass_right = cass_left + cass_w
    cass_bot = cass_top + cass_h

    # Cassette Drop Shadow inside squircle
    cass_shadow = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    cs_draw = ImageDraw.Draw(cass_shadow)
    cs_draw.rounded_rectangle(
        [cass_left - 4, cass_top + 12, cass_right + 4, cass_bot + 16],
        radius=24,
        fill=(0, 0, 0, 160)
    )
    cass_shadow = cass_shadow.filter(ImageFilter.GaussianBlur(14))
    master_img.paste(cass_shadow, (0, 0), cass_shadow)

    # Cassette Body (Matte Studio Anthracite)
    draw.rounded_rectangle(
        [cass_left, cass_top, cass_right, cass_bot],
        radius=24,
        fill=(24, 25, 30, 255),
        outline=(62, 66, 78, 255),
        width=4
    )

    # Corner Screws
    screw_r = 6
    screws = [
        (cass_left + 22, cass_top + 22),
        (cass_right - 22, cass_top + 22),
        (cass_left + 22, cass_bot - 22),
        (cass_right - 22, cass_bot - 22),
        (cx, cass_bot - 22)
    ]
    for sx, sy in screws:
        draw.ellipse([sx - screw_r, sy - screw_r, sx + screw_r, sy + screw_r], fill=(68, 72, 85, 255), outline=(18, 20, 24, 255), width=2)
        draw.line([sx - 3, sy - 3, sx + 3, sy + 3], fill=(28, 30, 36, 255), width=2)

    # Vintage Gold Label Area
    lbl_pad_x = 44
    lbl_pad_y = 34
    lbl_rect = [cass_left + lbl_pad_x, cass_top + lbl_pad_y, cass_right - lbl_pad_x, cass_bot - lbl_pad_y - 40]
    draw.rounded_rectangle(lbl_rect, radius=14, fill=(218, 172, 85, 255), outline=(155, 120, 50, 255), width=3)

    # Label Studio Racing Stripes (Burgundy & Deep Blue)
    strip_y = lbl_rect[1] + 30
    draw.rectangle([lbl_rect[0] + 6, strip_y, lbl_rect[2] - 6, strip_y + 7], fill=(195, 42, 42, 255))
    draw.rectangle([lbl_rect[0] + 6, strip_y + 9, lbl_rect[2] - 6, strip_y + 14], fill=(30, 50, 90, 255))

    # Center Transparent Window
    win_w = 250
    win_h = 120
    win_left = cx - win_w // 2
    win_top = cy - win_h // 2 - 4
    win_right = win_left + win_w
    win_bot = win_top + win_h
    draw.rounded_rectangle([win_left, win_top, win_right, win_bot], radius=12, fill=(16, 17, 21, 255), outline=(75, 80, 95, 255), width=3)

    # Spools Left & Right
    spool_r = 42
    spool_lx = cx - 70
    spool_rx = cx + 70
    spool_y = cy - 4

    # Brown Tape Packs
    draw.ellipse([spool_lx - 58, spool_y - 58, spool_lx + 58, spool_y + 58], fill=(68, 38, 22, 255))
    draw.ellipse([spool_rx - 50, spool_y - 50, spool_rx + 50, spool_y + 50], fill=(68, 38, 22, 255))

    # White Hubs & Teeth
    for sp_x in [spool_lx, spool_rx]:
        draw.ellipse([sp_x - spool_r, spool_y - spool_r, sp_x + spool_r, spool_y + spool_r], fill=(242, 244, 248, 255), outline=(175, 180, 192, 255), width=2)
        # Center hole
        hole_r = 15
        draw.ellipse([sp_x - hole_r, spool_y - hole_r, sp_x + hole_r, spool_y + hole_r], fill=(16, 17, 21, 255))
        for i in range(6):
            ang = i * (math.pi / 3)
            tx = sp_x + math.cos(ang) * 21
            ty = spool_y + math.sin(ang) * 21
            draw.ellipse([tx - 4, ty - 4, tx + 4, ty + 4], fill=(175, 180, 192, 255))

    # Tape Guide Roller Trapezoid
    trap_pts = [
        (cx - 135, cass_bot),
        (cx + 135, cass_bot),
        (cx + 105, cass_bot - 42),
        (cx - 105, cass_bot - 42),
    ]
    draw.polygon(trap_pts, fill=(32, 34, 40, 255), outline=(60, 64, 76, 255))
    # Bronze rollers
    draw.ellipse([cx - 85 - 9, cass_bot - 23 - 9, cx - 85 + 9, cass_bot - 23 + 9], fill=(195, 145, 65, 255))
    draw.ellipse([cx + 85 - 9, cass_bot - 23 - 9, cx + 85 + 9, cass_bot - 23 + 9], fill=(195, 145, 65, 255))

    # --- 4. Glowing Studio Audio Waveform below Cassette ---
    wave_y = 765
    wave_bars = [14, 28, 44, 65, 86, 105, 80, 100, 112, 85, 62, 92, 106, 70, 48, 30, 16]
    n_bars = len(wave_bars)
    bar_span = 460
    step = bar_span / (n_bars - 1)
    bx_start = cx - bar_span // 2

    for i, h in enumerate(wave_bars):
        bx = bx_start + i * step
        frac = i / (n_bars - 1)
        r = int(56 + frac * 90)
        g = int(189 - frac * 50)
        b = int(248)
        draw.line([(bx, wave_y - h // 2), (bx, wave_y + h // 2)], fill=(r, g, b, 240), width=7)

    # --- 5. Export Assets ---
    os.makedirs("assets", exist_ok=True)
    master_img.save("assets/AppIcon.png")

    # Export 128x128 raw RGBA for fallback non-bundle embedding
    icon_128 = master_img.resize((128, 128), Image.Resampling.LANCZOS)
    with open("assets/icon_128.rgba", "wb") as f:
        f.write(icon_128.tobytes())

    # --- 6. Export macOS .iconset for iconutil ---
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
        resized = master_img.resize((s, s), Image.Resampling.LANCZOS)
        resized.save(os.path.join(iconset_dir, name))

    # Run iconutil to create AppIcon.icns
    subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", "AppIcon.icns"], check=True)
    print("Successfully generated Apple HIG compliant AppIcon.icns and assets/icon_128.rgba!")

if __name__ == "__main__":
    create_mac_app_icon()
