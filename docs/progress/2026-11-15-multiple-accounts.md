# Multiple accounts, phase 1 (#82)

Branch `feature/multiple-accounts`, based on `main` after #83.

## Behaviour

- **Storage.** Each account has its own OS credential slot (same service,
  account name `session-<store id>`). A non-secret index,
  `accounts.json` in the app data directory, lists the saved accounts (store
  ID, user ID, homeserver), their order and the one used last. It is written
  atomically (temporary file and rename) and holds no tokens or passphrases.
- **Migration.** At launch the original single slot (`active-session-v1`) is
  copied into its per-account slot, listed in the index, and only then
  removed; an interruption repeats the step. Sign-ins a crash interrupted
  (listed as pending) have their slot and store removed at the next launch.
- **Adding.** Sign-in (password, OAuth, SSO) always adds an account; the
  former "a saved session already exists" refusal is gone. Signing in a user
  that is already saved is refused after login, and the new device is
  revoked and its store deleted.
- **Switching.** `switch_account` stops the open account's sync,
  notifications and tasks, opens the other store and makes it the launch
  account. It is refused during a call. Stale snapshots are rejected by
  `account_instance` as before.
- **Signing out** removes only that account's slot, store and index entry,
  then opens the next saved account, or the sign-in screen if none is left.
  The same applies to discarding a revoked session.
- **Only the open account syncs.** Inactive accounts receive no
  notifications in this phase.

## UI

- Avatar menu: the saved accounts at the top with a checkmark on the open
  one (shown once there are two or more), then **Add Account…**; at the
  bottom **Sign Out of <name>…**, using the local part unless two saved
  accounts share it.
- **Add Account…** opens the existing sign-in flow as a sheet over the dimmed
  workspace: no brand mark, a sheet-sized title, Cancel instead of Back on the
  server step and next to Sign in on the method step. Escape cancels; outside
  clicks do not. Tab stays inside the sheet.
- **Sign Out** opens a confirmation dialog naming the user, warning when
  secure backup is off, and saying that other accounts stay signed in.
- Previews: `--preview-add-account`, `--preview-add-account-options`,
  `--preview-sign-out`.

## Verification

- Compile: `cargo fmt --check` and strict all-target, all-feature Clippy pass.
- Offline tests: 157 app tests, including the sheet (Escape and Cancel), the
  options step, the sign-out dialog and menu labels. Core unit tests cover
  the index (launch choice, duplicate refusal, atomic update, no credential
  fields).
- Local OS keychain (isolated service): the migration and interrupted
  sign-in cleanup test passes.
- Local Synapse (`run.sh keychain`): the existing cross-process
  login, restore and logout test passes on the per-account slots. A new test
  adds two accounts, refuses a duplicate, opens either one, remembers the
  switch, and signs one out while the other stays valid.
- Captures of the sheet (both steps) and the sign-out dialog reviewed.

Not verified: the menu switch in a running app against a live account, E2EE
on each account after switching, Element, and keychains on Linux (Secret
Service) and Windows (Credential Manager). The migration has not yet run on
the real saved session; it will on the first launch of this build.
