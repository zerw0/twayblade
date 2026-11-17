# Media cache and data saving (#174)

## Built

- **Settings › Chat › Media** (new group):
  - **Download images**: Always / **Unmetered Only** (default) / Never.
  - **Keep on this Mac** (other systems: "this computer"): Off, 250 MB,
    **500 MB** (default), 1 GB, 2 GB, 5 GB.
  - **Cached media**: the space the cache takes, and **Clear Cache**.
  - Reset to Defaults applies the defaults to the open account too (it now
    also re-applies the image size, which it previously only saved).
- Cache: images, timeline previews and gallery thumbnails are kept in the
  account's SDK media store, which encrypts content with the store key held
  in OS secret storage. No decrypted files are written. The limit is the
  store's retention policy; entries unused for 30 days expire; one file
  cannot take the whole cache. Off keeps media in memory only, as before.
  Clear Cache removes everything and compacts the store. Avatars follow the
  same limit. Saving an attachment to a file still bypasses the cache.
- Data saving: when automatic download is not allowed, timeline previews
  are read from the cache only. An image that is not there shows
  **Load Image**, which downloads that one image. Opening an image in the
  viewer always downloads it.
- Metered detection (`twayblade_platform::network::metered`, polled every
  15 s; a change re-plans visible previews):
  - macOS: Network framework path is expensive (cellular, personal hotspot)
    or constrained (Low Data Mode).
  - Linux: NetworkManager's `Metered` property (yes or guessed yes).
  - Windows: the internet profile's connection cost (fixed or variable plan,
    roaming, near or over the data limit).
  - Unknown counts as unmetered.

## Verified

- Compile: strict workspace Clippy on macOS. Offline tests: download
  choices against metering, cache limits, size wording, the not-downloaded
  mark until a preview arrives, and a UI test for Load Image. The app suite
  needs `RUST_MIN_STACK=8388608` locally (as CI sets it): one existing
  add-account render test now overflows the 2 MiB default test-thread stack
  in debug builds.
- macOS metered reading on this Mac: `Some(false)` on its normal network.
- Local Synapse: `viewer` checks that with the cache off a cache-only
  preview is not downloaded; with it on a download is kept and then served
  without the network, the cache reports a size, and Clear Cache empties
  it. `media` passes.

## Not verified

- Windows and Linux builds of the metered check (only compiled in CI;
  Windows API names checked against the `windows` 0.62.2 source); real
  cellular, hotspot or Low Data Mode switching on a Mac; a live account.
