# T3 Code Rust

Build a native Rust/egui client for the existing T3 Code server. Visual and
behavioral parity with the pinned upstream version is the goal. Do not redesign
the interface, replace egui with a webview, or port the server to Rust.

Reference: T3 Code v0.1.0, orchestration protocol 2. The interface was ported
from v0.0.45; port v0.1.0's changes from its source. The installed package
ships source maps with the original client sources; extract them for reading
(never vendor them into this repository).

- English code and documentation; French discussion.
- Preserve upstream labels, icons, geometry, and theme tokens. Port logic from
  the upstream source rather than guessing, and name the upstream file in the
  module docs.
- Record incomplete parity honestly. A compiled mockup is not a completed port.
- Use disposable fixtures for tests (`scripts/fixture-server.sh`); do not modify
  existing T3 data or sessions.
- Never discover, read, log, or persist user credentials. Authentication is
  entered by the user at runtime and stays in memory.
- Never run provider turns during automated checks: sending a message starts a
  real provider session with the user's provider accounts.
- No commits or pushes without permission in this repository.
- Check: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`. Compare rendered UI with `examples/snapshot.rs` against
  `scripts/reference/shot.sh` on the same fixture.
