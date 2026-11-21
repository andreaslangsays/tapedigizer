---
name: Obsidian Waveform
colors:
  surface: '#121316'
  surface-dim: '#121316'
  surface-bright: '#38393c'
  surface-container-lowest: '#0d0e11'
  surface-container-low: '#1b1b1f'
  surface-container: '#1f1f23'
  surface-container-high: '#292a2d'
  surface-container-highest: '#343538'
  on-surface: '#e3e2e6'
  on-surface-variant: '#d8c3ad'
  inverse-surface: '#e3e2e6'
  inverse-on-surface: '#2f3034'
  outline: '#a08e7a'
  outline-variant: '#534434'
  surface-tint: '#ffb95f'
  primary: '#ffc174'
  on-primary: '#472a00'
  primary-container: '#f59e0b'
  on-primary-container: '#613b00'
  inverse-primary: '#855300'
  secondary: '#4edea3'
  on-secondary: '#003824'
  secondary-container: '#00a572'
  on-secondary-container: '#00311f'
  tertiary: '#ffbcb7'
  on-tertiary: '#68000a'
  tertiary-container: '#ff938c'
  on-tertiary-container: '#8d0012'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#ffddb8'
  primary-fixed-dim: '#ffb95f'
  on-primary-fixed: '#2a1700'
  on-primary-fixed-variant: '#653e00'
  secondary-fixed: '#6ffbbe'
  secondary-fixed-dim: '#4edea3'
  on-secondary-fixed: '#002113'
  on-secondary-fixed-variant: '#005236'
  tertiary-fixed: '#ffdad7'
  tertiary-fixed-dim: '#ffb3ad'
  on-tertiary-fixed: '#410004'
  on-tertiary-fixed-variant: '#930013'
  background: '#121316'
  on-background: '#e3e2e6'
  surface-variant: '#343538'
typography:
  headline-lg:
    fontFamily: Inter
    fontSize: 24px
    fontWeight: '600'
    lineHeight: 32px
    letterSpacing: -0.015em
  headline-md:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: -0.01em
  headline-sm:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '600'
    lineHeight: 20px
    letterSpacing: -0.005em
  body-lg:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
  body-md:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '400'
    lineHeight: 16px
  body-sm:
    fontFamily: Inter
    fontSize: 11px
    fontWeight: '400'
    lineHeight: 14px
  label-lg:
    fontFamily: JetBrains Mono
    fontSize: 13px
    fontWeight: '500'
    lineHeight: 16px
    letterSpacing: 0.02em
  label-md:
    fontFamily: JetBrains Mono
    fontSize: 11px
    fontWeight: '500'
    lineHeight: 14px
    letterSpacing: 0.03em
  label-sm:
    fontFamily: JetBrains Mono
    fontSize: 9px
    fontWeight: '600'
    lineHeight: 12px
    letterSpacing: 0.05em
  counter-display:
    fontFamily: JetBrains Mono
    fontSize: 28px
    fontWeight: '700'
    lineHeight: 32px
    letterSpacing: 0.04em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 0.25rem
  margin: 0.5rem
  space-xs: 0.125rem
  space-sm: 0.25rem
  space-md: 0.5rem
  space-lg: 0.75rem
  space-xl: 1rem
---

## Brand & Style

This design system defines an uncompromising, precision-engineered digital audio workstation and analog digitization suite. The personality merges clinical broadcast metering with the tactile, organic heritage of analog audio equipment—reel-to-reel master decks, high-end mastering consoles, and vintage broadcast consoles.

The visual style is **Tactile Precision Dark**:
- An engineered, dark-slate visual architecture tailored for sustained low-light studio sessions.
- Physical cues rooted in hardware control surfaces: machined aluminum fader caps, knurled perimeter rings on encoders, crisp bezel dividers, and phosphor tube indicators.
- Uncompromising utilitarian feedback: every indicator, dial tick, and tick-mark adheres strictly to audio engineering standards (IEC, VU, true peak dBFS).
- High visual density with micro-radius framing and razor-thin mechanical separation lines, avoiding generic SaaS emptiness in favor of information-dense, latency-critical productivity.

## Colors

The palette directly communicates functional signal states through calibrated luminance:

- **Chassis & Matte Substrates (Neutrals):**
  - App Shell / Base Layer: `#121316` (Deep graphite void).
  - Deck Surfaces / Module Frames: `#1A1C22` (Matte anodized slate).
  - Recessed Wells / Troughs: `#0D0E11` (Waveform canvases, fader tracks, routing pits).
  - Component Faceplates / Active Panels: `#23262F` (Milled plate aluminum).
  - Boundary Hairlines: `#2E323D` (Subtle 1px edge reflections).

- **Analog Warmth & Engine Core (Primary - `#F59E0B`):**
  - Vintage warm amber represents active signal paths, playheads, analog saturation engagement, tape bias indicators, and active digitization passes.

- **Phosphor Metric & Safe Signal (Secondary - `#10B981`):**
  - Calibrated phosphor green for VU meters (-inf to -6 dBFS), active phase alignment, safe peak readouts, and active sample-rate sync locks.

- **Critical Saturation & Peak Clip (Tertiary - `#EF4444`):**
  - High-visibility incandescent red/orange alert reserved exclusively for true-peak overload (0 dBFS+ clipping), destructive sample overwrite warnings, and active hardware input clipping.

- **Data & Readout Accents:**
  - Cyan (`#06B6D4`): Used strictly for automation splines, auxiliary spectral overlays, and secondary stereo width indicators.

## Typography

The typographic hierarchy differentiates structural human interface interaction from high-throughput numerical telemetry.

- **Primary Interface (Inter):** Applied across menu systems, track headers, context popups, preferences, and module descriptors. Employs condensed optical sizing and deliberate negative tracking at larger sizes to retain legibility within densely packed dock panels.
- **Precision Telemetry (JetBrains Mono):** Mandated for timecode counters (`SMPTE: HH:MM:SS:FF`), sample counts, buffer latencies, frequency readouts (`Hz / kHz`), dynamic range (`LUFS`), and gain values (`dBFS`). Monospaced figures ensure jitter-free optical stability during real-time waveform playback.
- **Labels & Micro-Indicators:** Set strictly in uppercase with extended letter-spacing (`label-sm`) to emulate engraved console silk-screening.

## Layout & Spacing

The layout model is a modular, high-density dock grid engineered for multi-monitor workstations and single-screen laptops. It rejects generous whitespace in favor of maximized canvas acreage for audio signals, timelines, and meter bridges.

- **Layout Hierarchy:**
  - **Unified Title & Transport Bar:** 38px fixed height with native window controls (macOS traffic lights padded 12px from left, or Windows/Linux top-right controls) integrated directly alongside global transport buttons, clock readouts, and CPU/DSP usage meters.
  - **Timeline & Waveform Canvas:** Fluid dynamic workspace utilizing CSS Grid sub-panels with zero-latency horizontal zoom and scroll sync.
  - **Dockable Inspector & Channel Strips:** Collapsible fixed-width panels (280px or 340px) docking to the left, right, or bottom.
- **Grid & Gutters:** Sub-panels are separated by hairline splitters (1px borders with 4px hit targets for resizing). Inner layout gaps strictly adhere to micro-increments: 2px (`space-xs`) between tightly paired parameter inputs, 4px (`space-sm`) between channel controls, and 8px (`space-md`) between discrete processing units.

## Elevation & Depth

Visual hierarchy uses physical depth cues found in audio hardware: recessed wells, flush anodized faceplates, and tactile raised controls.

- **Recessed Level (-1):** Waveform views, fader channels, and timeline tracks occupy sunken wells. Rendered with background `#0D0E11`, enclosed by a 1px inner boundary (`#000000`, 60% opacity) and an inner top drop-shadow (`inset 0 1px 3px rgba(0, 0, 0, 0.8)`).
- **Deck Level (0):** The primary chassis substrate (`#1A1C22`) and static panel housings. Divided purely through 1px border rules (`#2E323D`) without soft ambient blur shadows.
- **Raised Controls (+1):** Segmented buttons, rotary knobs, and fader caps use subtle top edge highlights (`inset 0 1px 0 rgba(255, 255, 255, 0.08)`) and directional drop shadows (`0 1px 3px rgba(0, 0, 0, 0.5)`), visually lifting them 1mm above the deck plate.
- **Floating Overlays & Menus (+2):** Contextual plugins, routing menus, and VST envelopes float above the master interface utilizing `#23262F` framed by a `#3E4352` edge, with a tight ambient shadow (`0 8px 24px rgba(0, 0, 0, 0.7)`). Glassmorphism is avoided across all audio processing planes to prevent GPU performance degradation during real-time rendering.

## Shapes

This design system uses a strict **Soft (`1`)** shape geometry:

- **Chassis Panels, Wells, & Toolbars:** 0px to 2px corner radius. Windows and major functional blocks rely on structural right angles to maximize every square pixel of display real estate.
- **Buttons, Segmented Toggles, & Enclosures:** 4px (`0.25rem`) standard outer corner radius, with 2px inner nest radius.
- **Fader Caps & Sliders:** Rectangular geometry with subtle horizontal bevel lines engraved across the cap center.
- **Meters & Signal Bars:** Strict 0px or 1px segmented rectangular blocks reflecting discrete LED display arrays.
- **Rotary Knobs:** Pure circular geometry (32px to 48px) with a 2px tick notch and a circular indicator ring spanning 270 degrees.

## Components

### Transport Bar & Segmented Toolbars
- **Container:** Solid `#1A1C22` background, 38px height, bordered by a bottom 1px `#2E323D` stroke.
- **Segmented Controls:** Milled `#23262F` button clusters with 1px dividers between states. Active states switch to `#2C313E` with an amber (`#F59E0B`) or phosphor green (`#10B981`) text/glyph glow (`0 0 8px rgba(245, 158, 11, 0.3)`).
- **Timecode Engine:** Centered high-contrast counter display (`#000000` well background, JetBrains Mono, `#F59E0B` typography).

### Rotary Encoders (Knobs) & Faders
- **Rotary Knob:** Dark radial gradient fill representing anodized metal, 1px perimeter ring with a radial gauge arc indicating value. The top dead center indicator mark uses high-luminance ivory (`#F3F4F6`), shifting to `#F59E0B` when modulated.
- **Console Faders:** Vertical channel well (`#0D0E11`, 4px wide) with a high-contrast center zero-line. Fader cap is a 24x12px textured graphite block with a 1px center hairline in amber or ivory.

### VU & Peak Level Meters
- **Structure:** Multi-segment segmented vertical or horizontal bars.
- **Color Thresholds:**
  - `-inf to -12 dBFS`: Solid phosphor green (`#10B981`).
  - `-12 to -2 dBFS`: Amber glow (`#F59E0B`).
  - `-2 dBFS to 0 dBFS`: Incandescent orange (`#F97316`).
  - `> 0 dBFS (Clip)`: Persistent sticky single-segment red pill (`#EF4444`) that stays illuminated until cleared by clicking.

### Waveform Display & Rulers
- **Timeline Canvas:** Pitch-black background (`#090A0C`). Grid lines spaced according to time/tempo subdivisions rendered in `#1E212A` at 1px width.
- **Waveform Render:** Precision vector canvas with high-contrast dual-color rendering: root signal in `#F59E0B` with an alpha fill (25% opacity) and 1px crisp outline at 100% opacity. Inverted stereo channels reflect symmetrically across the baseline.
- **Scrubber / Playhead:** 1px vertical hairline spanning full canvas height in `#F59E0B`, capped with an inverted triangle glyph at the ruler boundary.

### Inputs & Monospace Parameter Tickers
- **Micro-Value Scrubbers:** Numeric parameter fields (`JetBrains Mono`, 11px) double as bidirectional drag scrubbers. Clicking enables direct numeric entry; dragging modifies values incrementally.
- **Checkboxes & Solos:** Square 14px indicators. Standard checkboxes use a discrete phosphor green inner dot. Track "Mute" uses a muted red fill (`#7F1D1D`), and Track "Solo" illuminates in vivid amber (`#F59E0B`).