# Third-party notices

Termi depends on open-source Rust packages recorded exactly in `Cargo.lock`.
Release archives include a generated `THIRD_PARTY_NOTICES.html` containing the
package names, versions, license identifiers, and full detected license texts.

Regenerate that file with:

```bash
cargo install --locked --version 0.9.1 --features cli cargo-about
cargo about generate --locked --fail \
  --output-file THIRD_PARTY_NOTICES.html about.hbs
```

The interface includes Slint's `AboutSlint` widget. Slint 1.17.1 is available
under its published license choices; see the generated notice and the Slint
license files in the Cargo package for the complete terms.

This inventory is provided for attribution and distribution bookkeeping, not
as legal advice.
