# Checksum-verified update download (#56)

## Built

- For a direct-download install, Settings › General offers **Download
  Update** when the available release has this machine's installer and a
  `SHA256SUMS`:
  - macOS: the DMG for the CPU;
  - Windows x86_64: the MSI;
  - Linux x86_64: the AppImage, only when running from one.
- Package-managed installs keep showing their upgrade command and never
  download.
- The download streams into the Downloads folder with progress. It is kept
  only if its size and SHA-256 match the release; otherwise it is deleted
  and the reason shown (mismatch, no checksum, disk, network), with Try
  Again.
- **Open Installer** opens the DMG, runs the MSI, or shows the AppImage. It
  is disabled during a call. The app never installs or replaces itself.
- URLs are built from the validated tag and file name. Redirects are limited
  to GitHub's download hosts over HTTPS.
- The design and trust limits are in `docs/architecture/UPDATES.md`. The
  checksum detects a damaged download, not a compromised release.
- Preview: `--preview-update-download` (sample data).

## Verified

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy pass
  (macOS host).
- Offline tests:
  - Installer choice per OS and CPU, package managers excluded, hostile file
    names and missing checksums refused, `sha256sum` parsing.
  - A local HTTP server serves a matching and a mismatched checksum: the
    file is kept only for the match, and no partial file is left.
  - Settings states: offered, downloading, ready, failed, and
    package-managed.
- Capture of the Ready state reviewed.

## Not verified

- A real release: none is published, so no download from GitHub has run.
- Opening a DMG, MSI or AppImage from the app on a physical machine.
- Linux and Windows builds beyond the shared lint and test gates.
- A launch-time update notice outside Settings is still not built.
