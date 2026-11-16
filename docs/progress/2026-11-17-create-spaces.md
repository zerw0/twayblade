# Create spaces (#169)

## Built

- The **+** at the bottom of the space rail opens a menu: **Create Space…**
  and **Join a Space…**.
- Create Space asks for a name, an optional description and who can join:
  **Private** (only people you invite, the default) or **Public** (anyone can
  find and join it). The new space opens in the rail.
- Core: `Account::create_space` creates a room of type space (public or
  private preset); as in Element, only admins post in the space room itself
  and members may invite. A space needs a name.
- Preview: `--preview-create-space`.

## Verified

- Compile: strict workspace Clippy. Offline tests: core validation unit test
  and a UI test (private by default, switching to public).
- Local Synapse: `run.sh spaces` creates a public space, sees it joined as a
  space with a public join rule and admin-only posting, and refuses a space
  without a name. The profile then stops at "create voice channel", which
  fails on plain develop too (#176).
- Capture of the dialog reviewed.

## Not verified

- A live account; adding a picture to the new space (not offered yet).
