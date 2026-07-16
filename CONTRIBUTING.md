# Contributing to Termi

Termi welcomes focused bug fixes, tests, documentation, compatibility work,
and performance improvements that fit the supported Linux target. Before
starting a large feature, open an issue so scope and terminal-compatibility
tradeoffs can be discussed.

## Development target

The production baseline is Ubuntu 24.04 x86_64 on Wayland and X11. The checked
in `rust-toolchain.toml` selects Rust 1.92.0 with Rustfmt and Clippy. Install the
system packages listed in `README.md`, then run:

```bash
./scripts/check.sh
cargo run --locked
```

Install `cargo-deny` when changing dependencies or release policy:

```bash
cargo install --locked cargo-deny
cargo deny --locked check advisories sources
```

See `docs/TESTING.md` for the automated and manual matrix.

## Making a change

1. Keep each change narrow enough to review and verify.
2. Add regression tests for parser, keyboard, configuration, queue, or
   sanitization changes whenever the behavior can be exercised without a GUI.
3. Test UI or window-system changes on every affected backend.
4. Update `README.md`, `product.md`, `docs/ARCHITECTURE.md`, or
   `docs/TESTING.md` when the product contract changes.
5. Add a concise entry under `CHANGELOG.md`'s Unreleased section for
   user-visible, compatibility, packaging, or security changes.
6. Run the complete checks before opening a pull request.

Do not commit `target/`, `dist/`, generated `THIRD_PARTY_NOTICES.html`, local
editor state, logs, credentials, terminal history, private paths, or screenshots
containing sensitive terminal content.

## Terminal-specific review points

Terminal changes should consider all of the following:

- normal and alternate screens;
- application and normal cursor modes;
- bracketed and ordinary paste;
- Unicode scalar values, combining behavior, and wide cells;
- default, indexed, and true colors plus inverse attributes;
- scrollback position, selection, and search interaction;
- X10, VT200, button-motion, and any-motion pointer reporting;
- bounded memory and non-blocking behavior under hostile or high-volume output;
- shell exit, tab close, window close, and worker-thread cleanup;
- local versus remote OSC 7 directory metadata.

Avoid claiming broad xterm compatibility from one application test. Document
the exact sequence or application being fixed and add a focused regression
where possible.

## Dependency changes

Keep direct dependencies minimal and locked. For every dependency change:

- explain why the crate or feature is needed;
- prefer the narrowest feature set;
- run `cargo deny --locked check advisories sources`;
- regenerate third-party notices with the command in `README.md`;
- confirm the license is compatible with the repository and Slint distribution
  obligations;
- rebuild and verify all three release packages when the runtime graph changes.

Exceptions in `deny.toml` must be exact, have a concrete reachability reason,
and be removed as soon as a compatible upstream version is available.

## Pull requests

A ready pull request has a clear summary, links its issue when applicable,
lists exact verification performed, calls out deferred testing, and contains no
unrelated formatting churn. CI must pass on the minimum Rust toolchain.

By contributing, you agree that your contribution is licensed under the MIT
License in `LICENSE`.
