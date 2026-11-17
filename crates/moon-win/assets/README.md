# Embedded symbols font

`SymbolsNerdFont-Regular.ttf` is the unmodified Symbols Nerd Font from Nerd Fonts
**v3.5.1**, verified against the current upstream release on 2026-11-11.

- Source: https://github.com/ryanoasis/nerd-fonts/tree/v3.5.1/patched-fonts/NerdFontsSymbolsOnly
- The upstream assembly notice is reproduced in [Symbols-LICENSE.txt](Symbols-LICENSE.txt). The bundled icon sets carry additional [glyph notices](Symbols-NOTICES.txt).
- SHA-256: `2839f0a572d4559f3f17a6fb74b8772e183f0c0a47150998ab194932cad55829`

The Settings frontend ships a byte-identical copy as `apps/settings/dist/moon-symbols.ttf`.
The font is embedded in the binary and loaded directly from its static bytes
into a private DirectWrite font collection. No installation, download or cache
file is needed at runtime. Private-use glyphs in bar text use this collection;
normal text uses the configured font with Windows' normal fallback behavior.
