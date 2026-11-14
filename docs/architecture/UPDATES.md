# Update notices and future installation

## Implemented contract

`platform::updates` owns a separate HTTP client with no Matrix client, account,
credentials, cookies or authentication headers. It requests only public releases
from `api.github.com/repos/zerw0/twayblade/releases`, using the GitHub REST API
version `2026-03-10`. Redirects are rejected; responses have a 15-second timeout
and 4 MiB limit. Changelogs are retained as text, at most 64 KiB on a Unicode
boundary. Nothing from a release body is logged or executed.

Stable is the default. It checks `/releases/latest`, rejects drafts and both
flagged and SemVer-tagged pre-releases. Explicit pre-release opt-in checks the
newest 100 published releases and chooses the greatest newer SemVer precedence,
including stable releases. Build metadata alone never counts as an upgrade.
A repository with no published release produces no notice. No GitHub account or
API token is required. See the [GitHub release API](https://docs.github.com/en/rest/releases/releases).

The backend starts checks at application launch, independently of sign-in, then
24 hours after each completed attempt. Enabling checks or changing channel
checks immediately. Disabling cancels in-flight work and clears the notice;
late results for the previous configuration are rejected. Identical preference
updates do not trigger another check. Network failures retry at the next daily
interval, without a busy retry loop. Dropping the service ends its work.

`Preferences.updates` contains `enabled` and `channel`; old preferences default
to enabled/stable. `Snapshot.updater` exposes typed phase/error, installation
method, last-attempt timestamp and optional `Available` data: version,
pre-release status, changelog, fixed-repository release URL and optional upgrade
command. Account replacement and sign-out preserve this global state.

The settings view should call
`MatrixApp::set_update_preferences(enabled, channel, window, cx)`: it applies the
setting immediately and uses the existing serialized preference writer. General
settings controls, update notices and What's New rendering are intentionally
unimplemented here. Treat changelog Markdown as untrusted content; never execute
it or automatically follow its links. Offline previews and backend fixtures
leave network checks disabled.

## Installation ownership

Detection is read-only. Package builders can stamp
`TWAYBLADE_PACKAGE_MANAGER=homebrew|deb|rpm|aur|nix|winget|direct` at compile time.
Otherwise the updater recognizes canonical Nix store, Homebrew Caskroom and
winget portable-package paths. Moved Homebrew apps are checked against the local
cask receipt. Linux checks ownership of the running executable with dpkg-query,
rpm or pacman; AUR classification additionally requires a foreign package.
Windows checks winget's installed-package registration. These local subprocesses
have a two-second budget, bounded output and no application credentials.
Unrecognized installations remain `Unknown`, not proven direct downloads.

| Manager | Displayed command |
| --- | --- |
| Homebrew | `brew upgrade --cask twayblade` |
| deb | `sudo apt install --only-upgrade twayblade` |
| rpm | `sudo dnf upgrade twayblade` |
| AUR | installed `paru` or `yay`, `-Syu`, and the owned whitelisted package name |
| Nix profile | `nix profile upgrade twayblade` |
| winget | `winget upgrade --id zerw0.Twayblade --exact` |

An AUR install without either helper exposes no invented command. NixOS and
nix-darwin declarative installations must update their configuration's flake
input and rebuild through their normal workflow; the profile command is for
profile installs. Package repositories/manifests can lag GitHub releases, so a
release notice does not prove that the manager already carries that version.
No displayed command is executed. Real installed Homebrew, distro, AUR, Nix and
winget ownership/upgrade acceptance remains required. Unknown ownership must
never authorize a future self-install.

## Signature and installation design (not implemented)

GitHub HTTPS, an API asset digest and a checksum file from the same release are
not an independent trust root. Direct-install updates need a detached Ed25519
signature over a versioned canonical manifest, verified with public keys pinned
in the installed application. Private release keys belong in the authorized
release-signing workflow, never in the app, repository or GitHub release body.
No signing credentials or keys have been created by this work.

The signed manifest must bind the channel, SemVer, release identity, target OS
and architecture, minimum supported OS/app/schema, archive URL, byte length and
SHA-256, publication/expiry times and signing-key ID. Define one canonical byte
encoding before implementation; sign those exact bytes. Reject wrong target,
expired metadata, stable-to-preview transitions without opt-in, rollback and
equal-version replacement. Validate the bounded archive bytes against both size
and hash before unpacking; reject traversal, escaping symlinks and decompression
limits. Download redirects must stay within the separately approved asset-origin
policy. A release page cannot supply its own new trusted public key.

On macOS, use a Sparkle-style EdDSA-signed update feed/manifest and independently
verify the app's Developer ID identity, signed bundle and notarization policy.
Windows must also verify Authenticode and expected publisher; Linux package
managers retain their own repository/package signature verification. Keep
package-managed installations on their managers, even if a signed direct asset
is available. These platform checks complement the pinned manifest signature.

Key rotation needs versioned, expiring trust metadata authenticated by the
existing trusted key, a documented overlap period and revocation/recovery policy.
Prefer an offline root with separate release keys; specify rollback protection
and an emergency root-key recovery procedure before enabling installation.

Installation must stage a verified artifact and replace atomically with a known
good rollback path. Failed verification leaves the current binary running.
Restart requires an explicit user action and must remain blocked during any
active/starting/reconnecting call. Preserve OS-backed credentials, encrypted SDK
stores and preferences; schema migration and rollback compatibility need their
own tests. A file swap is not session-restoration acceptance.

Remaining release acceptance: signed pre-release-to-pre-release upgrade on each
OS, tampered metadata/signature/archive rejection, interrupted download/replace,
wrong-target and rollback rejection, call restart guard, session/store retention
and package-manager upgrades. This groundwork downloads no release assets and
implements no verification, installation or restart action.
