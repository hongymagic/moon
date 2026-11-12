# Moon identity assets

The original crescent mark uses a 48-unit grid with a warm terracotta-to-linen
light treatment. It has a transparent background; small raster sizes use a
slightly fuller inner curve to preserve the tips. The geometric artwork is
created for this project and includes no third-party imagery or font glyphs.

`moon.svg` is the scalable master. `moon.ico` contains 32-bit RGBA images at
16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, and 256 pixels. This
covers the standard Windows sizes plus exact 125% and 150% tray/taskbar sizes.
`moon-tray-light` and `moon-tray-dark` provide dark and light monochrome marks
respectively for contrast-sensitive contexts. PNG companions support previews
and the settings surface.

Regenerate all assets with `python3 crates/moon-win/assets/icons/generate.py`.
The generator uses the Python standard library, fixed geometry, deterministic
supersampling, and no external image dependencies. Smaller ICO entries use
32-bit DIBs with alpha and an AND mask; the 256-pixel entry uses PNG compression.

The shared `crates/windows-resources.rs` writer embeds these icon groups:

| Resource ID | Asset |
| --- | --- |
| 101 | `moon.ico` — primary application mark |
| 102 | `moon-tray-light.ico` — dark monochrome |
| 103 | `moon-tray-dark.ico` — light monochrome |

Both executables also receive Unicode VERSIONINFO and their own manifests.
The settings companion always uses `asInvoker` with `uiAccess=false`.

References checked 2026-11-12:

- [Windows app icon construction and DPI sizes](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction)
- [Windows icon design guidance](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-design)
- [Win32 resource formats](https://learn.microsoft.com/en-us/windows/win32/menurc/resource-file-formats)
- [Version information resources](https://learn.microsoft.com/en-us/windows/win32/menurc/versioninfo-resource)
