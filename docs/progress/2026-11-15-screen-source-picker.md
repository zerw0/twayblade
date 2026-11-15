# Screen share source picker

Screen share source picker (2026-11-15, `feature/screen-source-picker`, issue #47):
where the OS has no picker of its own (X11 and Windows), Share opens a chooser in
the call stage: Screens and Windows tabs over the list core provides, refresh,
cancel, and picking a source starts sharing. macOS and Wayland keep the OS
picker. Verified by a capture with sample sources, an offline UI test and
compilation. Not verified: a real X11 or Windows session, the live source list
and capture, thumbnails (none are shown), or VoiceOver, Orca and Narrator.
