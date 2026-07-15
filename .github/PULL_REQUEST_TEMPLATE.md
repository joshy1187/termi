# Pull request

## Summary

Describe the user-visible or internal change and why it is needed.

## Verification

- [ ] `./scripts/check.sh`
- [ ] `cargo deny --locked check advisories sources`
- [ ] Relevant Wayland and/or X11 manual checks from `docs/TESTING.md`
- [ ] Documentation and `CHANGELOG.md` updated when behavior changed
- [ ] No generated files, release artifacts, credentials, or private data added

## Compatibility

Call out terminal-protocol, configuration, packaging, or minimum-Rust-version
effects. Include screenshots only when the visual surface changed.
