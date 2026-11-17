# Subspaces in the conversation list

Implemented on `feature/subspace-dropdowns`.

- Top-level spaces stay in the far-left rail. Joined subspaces appear with
  avatars and disclosure buttons inside the selected space's conversation list.
  Orphaned joined spaces remain reachable from the rail.
- Expanding a subspace shows its rooms and nested subspaces. Collapsing it keeps
  the open conversation selected. Search, history and notification navigation
  reveal containing groups when opening a room. Groups reset on account changes.
- Subspace context menus retain Browse rooms and Leave space. Directly shared
  rooms remain visible under each parent; archived rooms keep their Archive
  section. Rendering guards against cyclic ancestry and excessive nesting.
- Core snapshots expose immediate child links from SDK-cached state alongside
  inclusive descendant membership. SDK room metadata supplies joined spaces
  deeper than the SDK navigation filter's two levels. No membership or server
  state is changed by navigation.
- `--preview-subspaces` adds an offline fixture for nested disclosures.

Validation: Rust 1.99.1 on macOS, explicitly authorized instead of Nix for this
session. Verification:

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- Core space hierarchy tests: 5 passed, including cached direct links and deep
  joined-space metadata, plus existing leave/cycle regressions.
- Offline GPUI/model subspace tests: 2 passed, covering disclosures, nested
  groups, ancestor cycles, shared rooms, orphan navigation and selection.
- Offline navigation regressions passed outside the sandbox. An earlier
  sandboxed run was killed with SIGKILL before reporting results.
- Built and visually inspected the macOS `--preview-subspaces` capture at
  `/tmp/twayblade-subspaces.png`. Projects is expanded, Planning is collapsed,
  and neither subspace appears in the rail.
- Rebuilt the macOS development bundle and restarted the online app. Its
  process runs without preview arguments; this is launch evidence, not a live
  hierarchy interoperability acceptance check.

No live homeserver hierarchy changes, Element interoperability, Linux/Windows
execution, or physical accessibility acceptance were performed.
