<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/brand/pdflens-lockup-dark.svg">
    <img src="docs/brand/pdflens-lockup.svg" width="360" alt="PdfLens">
  </picture>
</p>

<p align="center">
  <b>A PDF reader that tells you what a word means, right where you read it.</b><br>
  <a href="https://github.com/nikhnigm/pdfLens/releases/latest"><b>Download for Windows</b></a> ·
  <a href="https://github.com/nikhnigm/pdfLens/issues">Report a problem</a>
</p>

## Install

1. Download `pdflens-<version>-windows-x64.msi` (or the `-portable.zip`) from the
   [latest release](https://github.com/nikhnigm/pdfLens/releases/latest).
2. Run it. The installer is not code-signed yet: if Windows shows **Windows protected your PC**,
   choose **More info ▸ Run anyway**.
3. Optional: check the download against `SHA256SUMS.txt` on the release.

## What PdfLens adds

### Define a word

1. Turn it on in **Preferences ▸ AI** and pick Claude Code, Codex or a custom command.
2. Select up to three words and press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> (or click
   **Define** on the selection bar).

A draggable popup shows a one-line meaning that fits the sentence. The words and their sentence go
to the program you picked, which may send them to an online model; PdfLens itself makes no network
requests for this.

### Updates

PdfLens checks for a new release when it starts (turn it off in Preferences), or any time with
**Help ▸ Check for updates**. Run the new `.msi` to upgrade in place; your settings are kept.

### Runs beside PdfCraft

PdfLens has its own name, executable and settings, so it installs next to an official PdfCraft
without replacing it.

## Origin

PdfLens is a fork of [PdfCraft](https://github.com/storytold/pdfcraft), the open-source PDF
workbench by the ArtCraft team. Every other feature comes from PdfCraft; see its README for the
full list. PdfLens is not made, sponsored or endorsed by the ArtCraft Team and carries none of the
ArtCraft marks.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors. Required notices are in
[NOTICE](NOTICE); bundled fonts, icons and other assets are listed with their licences in
[ATTRIBUTION.md](ATTRIBUTION.md).
