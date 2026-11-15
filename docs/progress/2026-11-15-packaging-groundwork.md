# Packaging groundwork and accessibility audit note

Packaging groundwork (2026-11-15, issues #53 and #51): `packaging/linux/` holds
a desktop entry and AppStream metadata for the future deb, rpm, AUR and
AppImage packages; `packaging/README.md` lists what is still open. The
`matrix:` handler is deliberately absent because the app does not accept a URI
on launch yet. Nothing was built, signed or published.

Accessibility (#51): the theme palette contrast tests already cover every scheme in
light, dark and high contrast, so no new contrast test was added. Focus order
and focus return after closing dialogs, the viewer and panels were not
audited, and no screen reader was run.
