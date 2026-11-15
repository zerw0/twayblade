# Welcome screen and server picker

Welcome screen (2026-11-15, `feature/camera-linux-windows`, UI feedback): the
signed-out start page is now a centred card with the app mark, name, a one-line
description and a Sign in button. Sign in opens a server step with four
suggested public servers as a radio list, an "Other server" field, Continue and
Back; the existing password, browser and SSO step is unchanged apart from
sitting in the same card. `--preview-signin` captures the welcome step and
`--preview-servers` the server step. Verified by captures and compilation. Not
verified: the live flow against a homeserver, keyboard navigation of the radio
rows, VoiceOver, and that the suggested servers still accept sign-ups or logins.
