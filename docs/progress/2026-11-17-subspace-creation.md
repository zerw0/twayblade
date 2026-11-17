# Create subspaces from the channel dialog

Implemented on `feature/subspace-creation` together with nested subspace navigation.
Rebased onto current `develop`; its standalone space creation remains available
with a separate backend action from subspace creation.

The existing + / Create Channel dialog now offers Subspace alongside Voice
channel. The two switches are mutually exclusive; neither enabled means a text
channel. The submit action reads Create Subspace for that choice. Name, optional
topic and selected-parent context are reused; message encryption controls are
hidden for space containers.

Core creates an `m.space` room through the Matrix SDK, with a restricted join
rule allowing members of the parent space. It refreshes parent state and checks
permission to send child links before creation, then uses SpaceService to link
the new space. Existing text and encrypted voice creation remain separate kinds.
A partial linking failure explicitly says the subspace already exists and must
not be created again. Successful creation keeps the parent selected and lets
normal sync populate the new dropdown.

Verification: `cargo check --locked -p twayblade` passed with Rust 1.99.1;
`cargo fmt --all` applied and `git diff --check` passed. No tests were run during initial implementation, per
the user's instruction to test only on request or before pushes. No live space
was created; server permissions, restricted joins, rendering and interoperability
still need acceptance. Linux/Windows execution was not checked.

Pre-push verification on the rebased feature:

- Formatting and workspace Clippy with warnings denied passed.
- Core tests filtered by `space`: 7 passed, 1 ignored (live integration).
- App/GPUI tests filtered by `space`: 15 passed, including new coverage of
  text/voice/subspace switching, parent selection and encryption visibility.
  Used `RUST_MIN_STACK=8388608`, matching CI; a prior run with the default
  thread stack aborted in the existing space-home UI test.
- Gitleaks found no secrets in the feature commit.

These offline checks do not establish live creation or platform acceptance.
